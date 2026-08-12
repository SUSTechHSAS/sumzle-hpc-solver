//! Brute-force search solver with pruning for Sumzle

use crate::evaluator::{evaluate_expression_solver_bytes, is_integer};
use crate::types::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};

/// Number of distinct characters a Sumzle expression can contain.
///
/// Public because it appears in [`SolutionSink::accept_aggregate`]'s signature,
/// which sinks outside this module implement.
pub const CHARSET_LEN: usize = 24;
const NO_CHAR: u8 = 0;
const INVALID_INDEX: u8 = u8::MAX;

const fn build_char_index() -> [u8; 256] {
    let mut table = [INVALID_INDEX; 256];
    table[b'0' as usize] = 0;
    table[b'1' as usize] = 1;
    table[b'2' as usize] = 2;
    table[b'3' as usize] = 3;
    table[b'4' as usize] = 4;
    table[b'5' as usize] = 5;
    table[b'6' as usize] = 6;
    table[b'7' as usize] = 7;
    table[b'8' as usize] = 8;
    table[b'9' as usize] = 9;
    table[b'+' as usize] = 10;
    table[b'-' as usize] = 11;
    table[b'*' as usize] = 12;
    table[b'/' as usize] = 13;
    table[b'%' as usize] = 14;
    table[b'^' as usize] = 15;
    table[b'=' as usize] = 16;
    table[b'(' as usize] = 17;
    table[b')' as usize] = 18;
    table[b'!' as usize] = 19;
    table[b'[' as usize] = 20;
    table[b']' as usize] = 21;
    table[b'>' as usize] = 22;
    table[b'A' as usize] = 23;
    table
}

const CHAR_INDEX: [u8; 256] = build_char_index();

/// Inverse of [`CHAR_INDEX`]: maps a charset index back to its ASCII byte.
/// Used to tie-break by character (ASCII) value so the top-N scoring matches
/// `server::compute_char_probabilities`, which orders ties by `char`.
const fn build_index_to_char() -> [u8; CHARSET_LEN] {
    let mut table = [0u8; CHARSET_LEN];
    let mut b = 0usize;
    while b < 256 {
        let idx = CHAR_INDEX[b];
        if idx != INVALID_INDEX {
            table[idx as usize] = b as u8;
        }
        b += 1;
    }
    table
}

const CHAR_FROM_INDEX: [u8; CHARSET_LEN] = build_index_to_char();

const FLOOR_NO_SLASH: &[u8] = b"0123456789/";
const FLOOR_WITH_SLASH: &[u8] = b"0123456789]";
const AFTER_EQ_START: &[u8] = b"-0123456789";
const AFTER_EQ: &[u8] = b"0123456789";
const FIRST_POSITION: &[u8] = b"123456789([";
const AFTER_DIGIT: &[u8] = b"0123456789+-*/%^A!)][=>";
const AFTER_BINARY_OR_OPEN: &[u8] = b"1234567890([";
const AFTER_CLOSE_OR_FACTORIAL: &[u8] = b"+-*/%^A!)][=>";
const DEFAULT_ORDER: &[u8] = b"1234567890+-*/=([)]%^!A>";
const END_CHARS: &[u8] = b"0123456789)]!";
const LENGTH_ONE_DIGITS: &[u8] = b"0123456789";

/// A destination for solutions discovered by the search.
///
/// The hot recursive search is generic over this trait so the default
/// in-memory path (`Vec<String>`) compiles to exactly the previous code
/// (monomorphized + inlined), while alternative sinks let the same search
/// stream solutions to disk or score them for top-N without materializing the
/// full solution set in memory. `accept` receives the completed expression as
/// raw bytes (guaranteed valid ASCII/UTF-8 by construction).
pub trait SolutionSink {
    fn accept(&mut self, expr: &[u8]);

    /// Accept a solution whose right-hand-side value the search already
    /// computed. Sinks that care about the value (only [`RhsCapture`]) override
    /// this to avoid re-evaluating the expression; everyone else ignores it.
    #[inline]
    fn accept_valued(&mut self, expr: &[u8], _rhs_value: i64) {
        self.accept(expr);
    }

    /// Whether this sink can absorb whole subtrees through
    /// [`accept_aggregate`](Self::accept_aggregate).
    ///
    /// Building the aggregate index costs one traversal of the right-hand-side
    /// set, which is wasted on a sink that needs the solutions themselves. This
    /// is a constant per sink type, so the check folds away at compile time and
    /// the index is only ever built for a solve that will use it.
    #[inline]
    fn wants_aggregate(&self) -> bool {
        false
    }

    /// Bulk-accept an entire right-hand-side subtree without enumerating it.
    ///
    /// Reports that `count` solutions share `prefix`, and that
    /// `suffix_char_counts[c]` of them contain character `c` somewhere in the
    /// suffix. Sinks that only need *aggregates* — how many solutions there
    /// are and which characters they use — can absorb a whole subtree in
    /// `O(CHARSET_LEN)` instead of `O(count)`.
    ///
    /// Returns `true` if the sink consumed the aggregate. The default is
    /// `false`: a sink that must see each solution individually (it emits,
    /// ranks, or stores them) declines and the search enumerates as usual.
    #[inline]
    fn accept_aggregate(
        &mut self,
        _prefix: &[u8],
        _count: u64,
        _suffix_char_counts: &[u64; CHARSET_LEN],
    ) -> bool {
        false
    }

    /// Pruning hook: may any completion of `prefix` using `remaining` further
    /// characters still be accepted?
    ///
    /// Returning `false` lets the search skip a whole subtree. Only
    /// [`TopNSink`] declines — it keeps a bounded set, so once the kept set is
    /// full a subtree whose *best possible* score cannot reach the weakest
    /// kept solution is dead weight. Every other sink accepts everything, so
    /// the default is `true` and the check inlines away for them.
    ///
    /// The search still reports the skipped expressions in its "searched"
    /// statistic, so pruning changes the run time and nothing else.
    #[inline]
    fn may_accept(&mut self, _prefix: &[u8], _remaining: usize) -> bool {
        true
    }

    /// Whether this sink ever declines a subtree. A constant so the pruning
    /// checks inside the hot RHS walk compile away entirely for the sinks that
    /// accept everything — they must not pay for a hook they never use.
    const PRUNES: bool = false;

    /// Prune hook for the RHS automaton, keyed on *character sets* instead of
    /// bytes.
    ///
    /// `present` is the set of characters already placed, `reachable` the set
    /// the remaining automaton can still produce, and `remaining` the number of
    /// characters left. Since the score depends only on which distinct
    /// characters appear, this is everything a bound needs — and knowing what
    /// the grammar can *actually* still emit makes the bound far tighter than
    /// one drawn from the whole charset.
    ///
    /// Only consulted when [`PRUNES`](Self::PRUNES) is set.
    #[inline]
    fn may_accept_masked(&mut self, _present: u32, _reachable: u32, _remaining: usize) -> bool {
        true
    }
}

impl SolutionSink for Vec<String> {
    #[inline]
    fn accept(&mut self, expr: &[u8]) {
        // Safety: the search only ever places bytes from the Sumzle charset,
        // all of which are valid single-byte UTF-8.
        let s = unsafe { std::str::from_utf8_unchecked(expr) };
        self.push(s.to_owned());
    }
}

/// Bitmask of the distinct charset indices present in `expr`. CHARSET_LEN is
/// 24, so a `u32` holds one bit per possible character.
#[inline]
fn unique_char_mask(expr: &[u8]) -> u32 {
    let mut mask = 0u32;
    for &ch in expr {
        mask |= 1u32 << idx_of(ch);
    }
    mask
}

/// Accumulates the statistics needed to compute character probabilities over
/// the full solution set without storing any solution: the total number of
/// solutions and, per charset index, how many solutions contain that character.
/// Mirrors `server::compute_char_probabilities`, which counts each character at
/// most once per solution.
#[derive(Clone)]
pub struct CountSink {
    pub total: u64,
    pub char_counts: [u64; CHARSET_LEN],
}

impl Default for CountSink {
    fn default() -> Self {
        Self {
            total: 0,
            char_counts: [0; CHARSET_LEN],
        }
    }
}

impl CountSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another sink's counts into this one (for parallel reduction).
    pub fn merge(&mut self, other: &CountSink) {
        self.total += other.total;
        for i in 0..CHARSET_LEN {
            self.char_counts[i] += other.char_counts[i];
        }
    }

    /// Per-character `(char, count)` pairs over the full solution set, one
    /// entry for each character that appears in at least one solution. The
    /// count is the number of solutions containing that character (counted at
    /// most once per solution), matching `server::compute_char_probabilities`.
    /// Used to report character probabilities across *all* solutions in top-N
    /// mode, where the individual solutions are never materialized.
    pub fn char_count_pairs(&self) -> Vec<(char, usize)> {
        (0..CHARSET_LEN)
            .filter(|&i| self.char_counts[i] > 0)
            .map(|i| (CHAR_FROM_INDEX[i] as char, self.char_counts[i] as usize))
            .collect()
    }

    /// Per-character probability (percentage of solutions containing the char),
    /// matching `server::compute_char_probabilities`.
    pub fn probabilities(&self) -> [f64; CHARSET_LEN] {
        let mut probs = [0.0f64; CHARSET_LEN];
        if self.total == 0 {
            return probs;
        }
        let total = self.total as f64;
        for (p, &c) in probs.iter_mut().zip(self.char_counts.iter()) {
            *p = (c as f64 / total) * 100.0;
        }
        probs
    }

    /// Mask of the five most probable characters, ranked by probability
    /// descending then character (ASCII) ascending — matching the `take(5)`
    /// over the list sorted by `server::compute_char_probabilities`, which
    /// breaks ties by `char`. (Charset-index order is *not* ASCII order, so the
    /// tie-break must use the actual byte value to stay consistent.)
    pub fn top5_mask(&self) -> u32 {
        let mut order: Vec<usize> = (0..CHARSET_LEN)
            .filter(|&i| self.char_counts[i] > 0)
            .collect();
        order.sort_by(|&a, &b| {
            self.char_counts[b]
                .cmp(&self.char_counts[a])
                .then_with(|| CHAR_FROM_INDEX[a].cmp(&CHAR_FROM_INDEX[b]))
        });
        let mut mask = 0u32;
        for &i in order.iter().take(5) {
            mask |= 1u32 << i;
        }
        mask
    }
}

impl SolutionSink for CountSink {
    #[inline]
    fn accept(&mut self, expr: &[u8]) {
        self.total += 1;
        let mut mask = unique_char_mask(expr);
        while mask != 0 {
            let i = mask.trailing_zeros() as usize;
            self.char_counts[i] += 1;
            mask &= mask - 1;
        }
    }

    #[inline]
    fn wants_aggregate(&self) -> bool {
        true
    }

    /// Absorb a whole subtree from its aggregate — this sink never looks at an
    /// individual solution, only at totals.
    ///
    /// A character counts once per solution, so for each character the tally
    /// is: every one of the `count` solutions if the prefix already contains
    /// it, otherwise only those whose suffix does.
    #[inline]
    fn accept_aggregate(
        &mut self,
        prefix: &[u8],
        count: u64,
        suffix_char_counts: &[u64; CHARSET_LEN],
    ) -> bool {
        if count == 0 {
            return true;
        }
        self.total += count;
        let prefix_mask = unique_char_mask(prefix);
        for (i, slot) in self.char_counts.iter_mut().enumerate() {
            *slot += if prefix_mask & (1u32 << i) != 0 {
                count
            } else {
                suffix_char_counts[i]
            };
        }
        true
    }
}

/// Scores each solution with the probability-based score from
/// `server::compute_recommended` (sum of unique-character probabilities, plus
/// 50 per character among the global top-5) and keeps only the `n`
/// highest-scoring solutions in a bounded min-heap. Memory is O(n) regardless
/// of the total solution count.
pub struct TopNSink {
    n: usize,
    probs: [f64; CHARSET_LEN],
    top5_mask: u32,
    /// Min-heap keyed on (score, expr) so the lowest-scoring kept solution is
    /// the cheapest to evict. `Reverse` turns the max-heap into a min-heap.
    heap: std::collections::BinaryHeap<std::cmp::Reverse<ScoredSolution>>,
    /// Per-character score contributions (`prob + 50` for a top-5 character),
    /// sorted descending. Used by [`may_accept`](SolutionSink::may_accept) to
    /// bound the best score any completion of a prefix could reach.
    ranked_gain: [(f64, u8); CHARSET_LEN],
    /// Best "weakest kept score" published by *any* worker, as `f64` bits.
    ///
    /// A worker whose heap is full holds `n` solutions scoring at least its own
    /// minimum, so the final merged top-`n` cannot contain anything below that
    /// minimum either — which makes one worker's threshold a valid pruning
    /// floor for all of them. Sharing it lets a thread that has not yet found
    /// good solutions prune with its neighbours' progress, instead of each
    /// thread having to rediscover a strong cutoff on its own.
    ///
    /// Read relaxed: a stale value only costs a missed prune, never a lost
    /// solution, and the kept set is still decided by the exact per-heap
    /// comparisons in [`push_scored`](Self::push_scored).
    floor: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
}

/// A solution paired with its score. The `Ord` impl encodes *keep priority*
/// for the bounded min-heap: a solution is "greater" (more worth keeping) when
/// it has a higher score, or — on a tie — a lexicographically smaller
/// expression. The min-heap therefore evicts the lowest-scoring solution and,
/// among equal scores, the lexicographically largest one. The kept set is thus
/// the top `n` under "score descending, expression ascending", as documented
/// for `into_sorted`. (NaN is impossible — scores are finite sums.)
#[derive(Clone, PartialEq)]
struct ScoredSolution {
    score: f64,
    expr: String,
}

impl Eq for ScoredSolution {}

impl Ord for ScoredSolution {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .total_cmp(&other.score)
            // Tie: the smaller expression ranks higher (kept in preference to a
            // larger one), so the min-heap evicts the larger expression first.
            .then_with(|| other.expr.cmp(&self.expr))
    }
}

impl PartialOrd for ScoredSolution {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl TopNSink {
    pub fn new(n: usize, probs: [f64; CHARSET_LEN], top5_mask: u32) -> Self {
        // Precompute what each character is worth the first time it appears,
        // ranked best-first, so the prefix bound is a running prefix sum.
        let mut ranked_gain = [(0.0f64, 0u8); CHARSET_LEN];
        for i in 0..CHARSET_LEN {
            let bonus = if top5_mask & (1u32 << i) != 0 {
                50.0
            } else {
                0.0
            };
            ranked_gain[i] = (probs[i] + bonus, i as u8);
        }
        ranked_gain.sort_by(|a, b| b.0.total_cmp(&a.0));

        Self {
            n,
            probs,
            top5_mask,
            heap: std::collections::BinaryHeap::new(),
            ranked_gain,
            floor: None,
        }
    }

    /// Share a pruning floor with the other workers of a parallel solve. See
    /// [`floor`](Self::floor); purely an optimization, the kept set is
    /// unchanged.
    pub fn with_shared_floor(
        mut self,
        floor: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        self.floor = Some(floor);
        self
    }

    /// Publish this sink's threshold to its peers (see
    /// [`floor`](Self::floor)). Used to seed the shared floor from solutions
    /// gathered before the parallel phase starts.
    pub fn publish_floor_now(&self) {
        self.publish_floor();
    }

    /// Publish this sink's weakest kept score if it beats what is already
    /// there. Only meaningful once the heap is full — before that the sink
    /// would still accept anything, so it has no floor to contribute.
    #[inline]
    fn publish_floor(&self) {
        if self.heap.len() < self.n {
            return;
        }
        let Some(floor) = self.floor.as_ref() else {
            return;
        };
        let Some(std::cmp::Reverse(min)) = self.heap.peek() else {
            return;
        };
        let mine = min.score;
        let mut cur = floor.load(std::sync::atomic::Ordering::Relaxed);
        loop {
            if f64::from_bits(cur) >= mine {
                return;
            }
            match floor.compare_exchange_weak(
                cur,
                mine.to_bits(),
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(observed) => cur = observed,
            }
        }
    }

    /// The strongest pruning threshold available: this sink's own weakest kept
    /// score, or a better one published by another worker. `None` while the
    /// heap is not yet full and no peer has published — nothing can be pruned.
    #[inline]
    fn prune_threshold(&self) -> Option<f64> {
        let own = if self.heap.len() < self.n {
            None
        } else {
            self.heap.peek().map(|std::cmp::Reverse(m)| m.score)
        };
        let shared = self.floor.as_ref().and_then(|f| {
            let v = f64::from_bits(f.load(std::sync::atomic::Ordering::Relaxed));
            if v == f64::NEG_INFINITY {
                None
            } else {
                Some(v)
            }
        });
        match (own, shared) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Upper bound on the score of any completion of `prefix` that adds at most
    /// `remaining` further characters.
    ///
    /// The score is a sum over the *distinct* characters present, so the bound
    /// is what the prefix already scores plus the `remaining` richest
    /// characters it does not yet contain. No completion can beat that: each
    /// added slot introduces at most one new distinct character, and the
    /// ranked list is sorted by contribution descending.
    #[inline]
    fn score_upper_bound(&self, prefix: &[u8], remaining: usize) -> f64 {
        // No knowledge of what the grammar can still emit: any character is
        // assumed reachable.
        self.score_upper_bound_masked(unique_char_mask(prefix), u32::MAX, remaining)
    }

    /// Upper bound given the characters already `present`, the set the
    /// remaining grammar can still `reachable`ly produce, and how many
    /// characters are left.
    ///
    /// Restricting the candidate gains to `reachable` is what makes this
    /// tighter than [`score_upper_bound`](Self::score_upper_bound): the generic
    /// bound credits a prefix with the richest characters in the whole charset,
    /// most of which the right-hand-side grammar cannot produce at that point
    /// (an operator position cannot yield a digit, a closed bracket cannot
    /// reopen, and so on). It remains an over-estimate — each remaining slot
    /// still contributes at most one new distinct character — so pruning
    /// against it cannot discard a solution that would have been kept.
    #[inline]
    fn score_upper_bound_masked(&self, present: u32, reachable: u32, remaining: usize) -> f64 {
        let mut bound = 0.0f64;
        let mut m = present;
        while m != 0 {
            let i = m.trailing_zeros() as usize;
            bound += self.probs[i];
            if self.top5_mask & (1u32 << i) != 0 {
                bound += 50.0;
            }
            m &= m - 1;
        }
        // Characters that could still be added: reachable, not already present.
        let candidates = reachable & !present;
        if candidates == 0 || remaining == 0 {
            return bound;
        }
        let mut added = 0usize;
        for &(gain, idx) in self.ranked_gain.iter() {
            if added == remaining || gain <= 0.0 {
                break;
            }
            if candidates & (1u32 << idx as u32) == 0 {
                continue;
            }
            bound += gain;
            added += 1;
        }
        bound
    }

    #[inline]
    fn score(&self, expr: &[u8]) -> f64 {
        let mut mask = unique_char_mask(expr);
        let mut score = 0.0f64;
        while mask != 0 {
            let i = mask.trailing_zeros() as usize;
            score += self.probs[i];
            if self.top5_mask & (1u32 << i) != 0 {
                score += 50.0;
            }
            mask &= mask - 1;
        }
        score
    }

    fn push_scored(&mut self, item: ScoredSolution) {
        if self.n == 0 {
            return;
        }
        if self.heap.len() < self.n {
            self.heap.push(std::cmp::Reverse(item));
        } else if let Some(std::cmp::Reverse(min)) = self.heap.peek() {
            if item > *min {
                self.heap.pop();
                self.heap.push(std::cmp::Reverse(item));
            }
        }
    }

    /// Whether a solution with this score/expression would be retained, decided
    /// without allocating. Mirrors `push_scored`'s keep condition exactly so the
    /// owned `String` is only built for solutions that actually survive — the
    /// vast majority of candidates in a large search are discarded here for free.
    #[inline]
    fn would_keep(&self, score: f64, expr: &[u8]) -> bool {
        if self.n == 0 {
            return false;
        }
        if self.heap.len() < self.n {
            return true;
        }
        match self.heap.peek() {
            Some(std::cmp::Reverse(min)) => match score.total_cmp(&min.score) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                // Tie on score: keep only if this expression is lexicographically
                // smaller than the current minimum's (the smaller expression is
                // preferred, matching `ScoredSolution`'s eviction order).
                std::cmp::Ordering::Equal => {
                    let s = unsafe { std::str::from_utf8_unchecked(expr) };
                    s < min.expr.as_str()
                }
            },
            None => true,
        }
    }

    /// Merge another sink's kept solutions into this one (parallel reduction).
    pub fn merge(&mut self, other: TopNSink) {
        for std::cmp::Reverse(item) in other.heap.into_vec() {
            self.push_scored(item);
        }
    }

    /// Consume the heap and return the kept solutions sorted by score
    /// descending, ties broken by expression ascending (deterministic).
    pub fn into_sorted(self) -> Vec<(f64, String)> {
        let mut items: Vec<ScoredSolution> =
            self.heap.into_vec().into_iter().map(|r| r.0).collect();
        // Sort explicitly by (score desc, expr asc) rather than relying on the
        // `Ord` impl, whose tie-break is reversed for the heap's eviction logic.
        items.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.expr.cmp(&b.expr))
        });
        items.into_iter().map(|s| (s.score, s.expr)).collect()
    }
}

impl SolutionSink for TopNSink {
    #[inline]
    fn accept(&mut self, expr: &[u8]) {
        let score = self.score(expr);
        // Skip the allocation entirely for solutions that cannot make the cut.
        if !self.would_keep(score, expr) {
            return;
        }
        let s = unsafe { std::str::from_utf8_unchecked(expr) };
        self.push_scored(ScoredSolution {
            score,
            expr: s.to_owned(),
        });
    }

    /// Skip a subtree whose best conceivable score cannot displace the weakest
    /// solution currently kept. Until the heap is full nothing can be pruned —
    /// every solution is still a candidate.
    #[inline]
    fn may_accept(&mut self, prefix: &[u8], remaining: usize) -> bool {
        self.publish_floor();
        match self.prune_threshold() {
            // Strictly-less is the only safe cut: on a tie the lexicographic
            // rule can still prefer a completion over the current minimum.
            Some(threshold) => self.score_upper_bound(prefix, remaining) >= threshold,
            None => true,
        }
    }

    const PRUNES: bool = true;

    /// Prune inside the right-hand side, not just at its boundary. Deliberately
    /// does *not* republish the shared floor: this runs once per automaton node
    /// rather than once per left-hand side, and an atomic store at that rate
    /// costs more than the sharpened bound saves. The floor is still refreshed
    /// at every boundary by [`may_accept`](Self::may_accept).
    #[inline]
    fn may_accept_masked(&mut self, present: u32, reachable: u32, remaining: usize) -> bool {
        match self.prune_threshold() {
            Some(threshold) => {
                self.score_upper_bound_masked(present, reachable, remaining) >= threshold
            }
            None => true,
        }
    }
}

/// Streams solutions to a writer as JSON Lines (one `{"solution":"..."}` per
/// line) through a local byte buffer, flushing to the shared writer only when
/// the buffer fills. This keeps memory bounded and limits lock contention to
/// one acquisition per buffer flush rather than one per solution.
///
/// Write errors are captured rather than panicked on (the crate builds with
/// `panic = "abort"`, so a panic mid-search would kill the process): the first
/// error is stored and surfaced by [`finish`](Self::finish), and further
/// solutions are dropped so the buffer cannot grow without bound after a
/// failure (e.g. a full disk).
pub struct JsonlSink<'a, W: std::io::Write> {
    writer: &'a std::sync::Mutex<W>,
    buf: Vec<u8>,
    flush_at: usize,
    count: u64,
    error: Option<std::io::Error>,
}

impl<'a, W: std::io::Write> JsonlSink<'a, W> {
    pub fn new(writer: &'a std::sync::Mutex<W>) -> Self {
        // `accept` appends a line and *then* checks `flush_at`, so a flush can
        // fire with the buffer already past `flush_at`. Keeping `flush_at` one
        // line below capacity guarantees that trailing append still fits, so
        // the buffer never reallocates and stays strictly bounded at 256 KiB.
        // A Sumzle solution line is at most a few dozen bytes — far under 1 KiB.
        const CAPACITY: usize = 256 * 1024;
        Self {
            writer,
            buf: Vec::with_capacity(CAPACITY),
            flush_at: CAPACITY - 1024,
            count: 0,
            error: None,
        }
    }

    fn flush_buf(&mut self) {
        if self.error.is_some() || self.buf.is_empty() {
            return;
        }
        match self.writer.lock() {
            Ok(mut guard) => {
                if let Err(e) = guard.write_all(&self.buf) {
                    self.error = Some(e);
                }
            }
            Err(_) => {
                self.error = Some(std::io::Error::other("solution writer mutex poisoned"));
            }
        }
        self.buf.clear();
    }

    /// Flush any remaining buffered bytes and return the number of solutions
    /// written by this sink, or the first write error encountered. Call once
    /// after the search ends.
    pub fn finish(mut self) -> std::io::Result<u64> {
        self.flush_buf();
        match self.error.take() {
            Some(e) => Err(e),
            None => Ok(self.count),
        }
    }
}

impl<W: std::io::Write> SolutionSink for JsonlSink<'_, W> {
    #[inline]
    fn accept(&mut self, expr: &[u8]) {
        // Once a write has failed, stop buffering so memory stays bounded; the
        // error is reported by `finish`.
        if self.error.is_some() {
            return;
        }
        // No Sumzle charset character requires JSON string escaping, so the
        // expression can be embedded verbatim between quotes.
        self.buf.extend_from_slice(b"{\"solution\":\"");
        self.buf.extend_from_slice(expr);
        self.buf.extend_from_slice(b"\"}\n");
        self.count += 1;
        if self.buf.len() >= self.flush_at {
            self.flush_buf();
        }
    }
}

/// Fallback budget for the cached right-hand-side tables when the machine's
/// available memory cannot be determined.
const RHS_CACHE_FALLBACK_BYTES: usize = 512 * 1024 * 1024;

/// Bytes of RAM this process may actually use.
///
/// `/proc/meminfo` describes the *host*, which inside a container is not a
/// limit the process is allowed to reach: this sandbox reports 96 GB available
/// while `memory.max` caps the cgroup at 8 GB, and crossing that gets the
/// process OOM-killed rather than throttled. So take the smaller of what the
/// host offers and what the cgroup still allows.
fn available_memory_bytes() -> Option<usize> {
    let host = meminfo_available_bytes();
    match cgroup_memory_headroom() {
        Some(headroom) => Some(host.map_or(headroom, |h| h.min(headroom))),
        None => host,
    }
}

fn meminfo_available_bytes() -> Option<usize> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

/// Bytes still allowed by the memory cgroup (limit minus current usage), or
/// `None` when unlimited/unreadable.
fn cgroup_memory_headroom() -> Option<usize> {
    let read = |path: &str| -> Option<usize> {
        let raw = std::fs::read_to_string(path).ok()?;
        let raw = raw.trim();
        if raw == "max" {
            return None;
        }
        raw.parse::<usize>().ok()
    };

    // cgroup v2, then v1. A v1 limit is "unlimited" as a huge sentinel value,
    // which the host figure will dominate anyway.
    let limit = read("/sys/fs/cgroup/memory.max")
        .or_else(|| read("/sys/fs/cgroup/memory/memory.limit_in_bytes"))?;
    let used = read("/sys/fs/cgroup/memory.current")
        .or_else(|| read("/sys/fs/cgroup/memory/memory.usage_in_bytes"))
        .unwrap_or(0);
    Some(limit.saturating_sub(used))
}

/// Total memory the cached right-hand-side tables may occupy.
///
/// The tables are the solver's only large allocation and are shared immutably
/// by every thread, so half of available RAM is a reasonable default; set
/// `SUMZLE_RHS_CACHE_MB` to pin it (0 disables the cache entirely). Positions
/// whose table does not fit fall back to plain recursion, so an over-long
/// expression degrades to baseline behavior instead of exhausting RAM.
fn rhs_cache_budget_bytes() -> usize {
    if let Ok(raw) = std::env::var("SUMZLE_RHS_CACHE_MB") {
        if let Ok(mb) = raw.trim().parse::<usize>() {
            return mb.saturating_mul(1024 * 1024);
        }
    }
    // Two thirds of the headroom. Reserving capacity as it is charged (see
    // `RhsCapture::reserve`) keeps real usage within a few percent of the
    // budget — a 4.5 GB budget measured 4.66 GB peak RSS — so the remaining
    // third is ample for the branch set, per-thread buffers and whatever
    // consumes the solutions.
    available_memory_bytes().map_or(RHS_CACHE_FALLBACK_BYTES, |avail| avail / 3 * 2)
}

/// Bytes one cached entry costs: its characters plus its value.
#[inline]
fn rhs_entry_cost(rhs_len: usize) -> usize {
    rhs_len + std::mem::size_of::<i64>()
}

/// Number of integer-valued expressions of each length, measured by
/// enumeration (index = length). This is the entry count of a cached RHS table,
/// and depends only on the RHS length — the grammar for a `>` right-hand side
/// reads nothing else.
const RHS_ENTRY_COUNTS: [u64; 11] = [
    0,
    10,
    31,
    590,
    3_211,
    39_335,
    278_203,
    2_884_541,
    22_978_704,
    221_404_570,
    1_800_000_000,
];

/// Growth factor per extra character, used beyond the measured table. The
/// measured ratios run 8-10x; 12 keeps the estimate conservative (over-
/// estimating only forgoes a table that would not have fit anyway).
const RHS_GROWTH_PER_CHAR: u64 = 12;

/// Whether a table for an RHS of `rhs_len` characters could fit in `budget`.
///
/// Deliberately optimistic at the boundary: the real capture still enforces the
/// budget exactly, so a wrong "yes" costs one wasted enumeration, while a wrong
/// "no" would silently forfeit the optimization.
fn rhs_table_can_fit(rhs_len: usize, budget: usize) -> bool {
    let entries = match RHS_ENTRY_COUNTS.get(rhs_len) {
        Some(&n) => n,
        None => {
            let extra = (rhs_len - (RHS_ENTRY_COUNTS.len() - 1)) as u32;
            RHS_ENTRY_COUNTS[RHS_ENTRY_COUNTS.len() - 1]
                .saturating_mul(RHS_GROWTH_PER_CHAR.saturating_pow(extra))
        }
    };
    entries.saturating_mul(rhs_entry_cost(rhs_len) as u64) <= budget as u64
}

/// Sink used to capture a right-hand-side subtree once, so it can be replayed
/// for every left-hand side instead of being re-enumerated. Records only the
/// RHS slice of each completed expression, in DFS order.
///
/// Captures for the branches of one table share a single `budget` counter,
/// drawn down in coarse reservations so the atomic stays cold in the hot path.
struct RhsCapture<'a> {
    start: usize,
    rhs_len: usize,
    bytes: Vec<u8>,
    values: Vec<i64>,
    min_value: i64,
    max_value: i64,
    /// Remaining shared allowance, in bytes, for the whole table.
    budget: &'a AtomicIsize,
    /// Allowance already claimed from `budget` but not yet spent.
    reserved: usize,
    /// Set once the table blew its budget (or an entry failed to re-evaluate);
    /// the table is then discarded and those positions fall back to recursion.
    overflow: bool,
}

/// Bytes claimed from the shared budget per reservation.
///
/// Every branch may hold up to this much claimed-but-unspent, so the whole
/// table over-reserves by at most `chunk x branches` while it is being built —
/// with a few hundred branches, 1 MiB chunks would over-reserve by hundreds of
/// megabytes and reject tables that comfortably fit. 64 KiB keeps that slack
/// negligible while still amortizing the atomic over thousands of entries.
const RHS_RESERVE_CHUNK: usize = 64 * 1024;

impl RhsCapture<'_> {
    #[inline]
    fn record(&mut self, expr: &[u8], value: i64) {
        if self.overflow {
            return;
        }
        let cost = rhs_entry_cost(self.rhs_len);
        if self.reserved < cost && !self.reserve(cost) {
            return;
        }
        self.reserved -= cost;

        self.bytes
            .extend_from_slice(&expr[self.start..self.start + self.rhs_len]);
        self.values.push(value);
        self.min_value = self.min_value.min(value);
        self.max_value = self.max_value.max(value);
    }

    /// Claim another chunk of the shared allowance and *materialize* it as
    /// `Vec` capacity. Returns false (and marks overflow) if nothing is left.
    ///
    /// Growing the vectors here rather than letting `push` do it is what keeps
    /// the budget honest: `Vec` doubles, so a table charged N bytes would
    /// otherwise reach 2N of real capacity mid-growth — and with every branch
    /// doubling at once, the peak overshoots the budget by far enough to be
    /// OOM-killed. Reserving exactly what was charged makes the accounted
    /// figure the true high-water mark.
    #[cold]
    fn reserve(&mut self, cost: usize) -> bool {
        let take = RHS_RESERVE_CHUNK.max(cost);
        // `fetch_sub` returns the value *before* the subtraction.
        if self.budget.fetch_sub(take as isize, Ordering::Relaxed) < take as isize {
            self.budget.fetch_add(take as isize, Ordering::Relaxed);
            self.overflow = true;
            self.bytes = Vec::new();
            self.values = Vec::new();
            return false;
        }
        self.reserved += take;

        // Split the newly claimed bytes between the two vectors in the same
        // proportion `rhs_entry_cost` charges them.
        let entries = take / cost.max(1);
        let need_bytes =
            (self.bytes.len() + entries * self.rhs_len).saturating_sub(self.bytes.capacity());
        if need_bytes > 0 {
            self.bytes.reserve_exact(need_bytes);
        }
        let need_values = (self.values.len() + entries).saturating_sub(self.values.capacity());
        if need_values > 0 {
            self.values.reserve_exact(need_values);
        }
        true
    }

    /// Freeze into a table part, releasing the slack `Vec` growth leaves behind
    /// so the accounted budget matches the real resident size.
    fn finish(mut self) -> Option<RhsPart> {
        // Hand back whatever was claimed but never spent, so the remaining
        // allowance reflects real usage rather than reservation slack.
        if self.reserved > 0 {
            self.budget
                .fetch_add(self.reserved as isize, Ordering::Relaxed);
            self.reserved = 0;
        }
        if self.overflow {
            return None;
        }
        self.bytes.shrink_to_fit();
        self.values.shrink_to_fit();
        Some(RhsPart {
            bytes: self.bytes,
            values: self.values,
            min_value: self.min_value,
            max_value: self.max_value,
        })
    }
}

impl SolutionSink for RhsCapture<'_> {
    #[inline]
    fn accept(&mut self, expr: &[u8]) {
        // The search routes every `>` terminal through `accept_valued`; this
        // path only exists to satisfy the trait, so it re-derives the value.
        match evaluate_expression_solver_bytes(&expr[self.start..self.start + self.rhs_len])
            .filter(|v| is_integer(*v))
        {
            Some(v) => self.record(expr, v as i64),
            None => self.overflow = true,
        }
    }

    #[inline]
    fn accept_valued(&mut self, expr: &[u8], rhs_value: i64) {
        self.record(expr, rhs_value);
    }
}

/// A sink that throws everything away — used where a search is run purely to
/// partition the tree and is asserted to reach no complete expression.
struct NullSink;

impl SolutionSink for NullSink {
    #[inline]
    fn accept(&mut self, _expr: &[u8]) {}
}

/// One branch's worth of a cached right-hand side, kept as its own allocation.
///
/// Parts are stored in branch order, which is DFS order, and each holds a whole
/// number of entries — so iterating parts in order, then entries within a part,
/// reproduces the serial DFS sequence exactly. Keeping them separate avoids the
/// doubled peak a final concatenation would cost on a multi-gigabyte table.
struct RhsPart {
    bytes: Vec<u8>,
    values: Vec<i64>,
    min_value: i64,
    max_value: i64,
}

/// A fully enumerated right-hand side of a `>` equation, for one main-operator
/// position.
///
/// # Why this is sound
///
/// When `>` is placed at index `k`, the state entering the RHS is always the
/// same regardless of what the left-hand side was:
///
/// * the bracket stack is empty — an LHS with an unclosed bracket fails to
///   evaluate, so `>` is never placed after one;
/// * the floor context is clean — `can_place_char` forbids a main operator
///   inside `[...]`;
/// * a second main operator is forbidden inside the RHS;
/// * `char_counts` only feeds count-based constraints, so the table is built
///   only when there are none (positional constraints are fine: they are
///   indexed by position, which is fixed for a given `k`).
///
/// Everything else the grammar consults is a function of `(index, length)`,
/// which is fixed once `k` is. So the RHS subtree is identical for every LHS
/// and can be enumerated once and replayed.
///
/// Entries are stored in **DFS order** — the exact order the recursive search
/// would have produced them — so replaying preserves solution ordering,
/// `found_count` and `searched_count` bit-for-bit.
struct RhsTable {
    rhs_len: usize,
    /// Number of complete expressions the subtree reaches, including those
    /// that fail to evaluate. This is what the search adds to `searched_count`.
    total_leaves: u64,
    /// DFS-ordered entries, split into one part per build branch.
    parts: Vec<RhsPart>,
    min_value: i64,
    max_value: i64,
}

impl RhsTable {
    /// Number of cached entries.
    fn len(&self) -> usize {
        self.parts.iter().map(|p| p.values.len()).sum()
    }

    /// Resident bytes this table occupies — what it was charged to the budget.
    fn mem_bytes(&self) -> usize {
        self.parts
            .iter()
            .map(|p| p.bytes.len() + p.values.len() * std::mem::size_of::<i64>())
            .sum()
    }

    /// Emit every solution formed by this LHS value and the cached RHS set,
    /// in the same order the recursive search would have produced them.
    #[inline]
    fn replay<S: SolutionSink>(
        &self,
        lhs_value: i64,
        expr: &mut [u8],
        start: usize,
        sink: &mut S,
        searched_count: &mut u64,
    ) {
        *searched_count += self.total_leaves;

        // Nothing in the table can satisfy `lhs > rhs`: skip the scan entirely.
        if lhs_value <= self.min_value {
            return;
        }

        let rhs_len = self.rhs_len;
        let end = start + rhs_len;

        // Every entry in the table qualifies: emit them all without consulting
        // a single bound. This is the common case for a long left-hand side.
        if lhs_value > self.max_value {
            for part in &self.parts {
                for chunk in part.bytes.chunks_exact(rhs_len) {
                    expr[start..end].copy_from_slice(chunk);
                    sink.accept(expr);
                }
            }
            return;
        }

        for part in &self.parts {
            // Per-part bounds prune whole branches of the table for free.
            if lhs_value <= part.min_value {
                continue;
            }
            if lhs_value > part.max_value {
                // Every entry in this part qualifies — skip the comparisons.
                for chunk in part.bytes.chunks_exact(rhs_len) {
                    expr[start..end].copy_from_slice(chunk);
                    sink.accept(expr);
                }
            } else {
                for (chunk, &value) in part.bytes.chunks_exact(rhs_len).zip(part.values.iter()) {
                    if lhs_value > value {
                        expr[start..end].copy_from_slice(chunk);
                        sink.accept(expr);
                    }
                }
            }
        }
    }
}

/// Memory budget (deduplicated DFA nodes) for the grammar cache described
/// below. A few hundred MB at L=12; the state space grows only linearly with
/// the RHS length, so this stays well under the cgroup even at very large
/// lengths.
const DFA_NODE_BUDGET: u64 = 30_000_000;
/// Rough upper bound on distinct grammar states per unit of RHS length — used
/// only to skip a DFA that would exceed [`DFA_NODE_BUDGET`] before building it.
const DFA_STATE_FACTOR: u64 = 350_000;

/// Node budget for the grammar DFA, overridable with `SUMZLE_RHS_DFA_NODES`
/// (0 disables the DFA entirely, falling back to plain recursion).
fn dfa_node_budget() -> u64 {
    if let Ok(raw) = std::env::var("SUMZLE_RHS_DFA_NODES") {
        if let Ok(n) = raw.trim().parse::<u64>() {
            return n;
        }
    }
    DFA_NODE_BUDGET
}
/// RHS lengths beyond this are not worth caching with the grammar DFA; the
/// uncached search is used instead (still memory-bounded).
const DFA_MAX_RHS_LEN: usize = 80;

/// How much more the index build costs than the index it produces.
///
/// The accumulator is a `HashMap` keyed by value, so each entry pays for a
/// hash, control byte and the table's load-factor slack, and the map itself
/// doubles as it grows. Charging the build a multiple of the packed size keeps
/// the transient peak — which is what the process actually has to survive —
/// inside the budget rather than only the finished structure.
const VALUE_INDEX_BUILD_OVERHEAD: usize = 4;

/// Largest right-hand-side subtree worth indexing, in leaves.
///
/// The index is built by one traversal of the subtree and then amortized over
/// every left-hand side that reaches it. Past this size the build itself
/// becomes the dominant cost, so those positions keep the ordinary replay path
/// — which the branch-and-bound prune already handles well.
const VALUE_INDEX_MAX_LEAVES: u64 = 200_000_000;

/// Memory allowed for the aggregate `lhs > rhs` indexes (see
/// [`RhsValueIndex`]), overridable with `SUMZLE_RHS_VALUE_INDEX_MB` (0
/// disables them). The default is deliberately small — the index is bounded by
/// the number of *distinct RHS values*, which is orders of magnitude below the
/// solution count, so a few hundred MB is generous even at very large lengths
/// and keeps the footprint compatible with the memory-bounded modes.
fn value_index_budget_bytes() -> usize {
    if let Ok(raw) = std::env::var("SUMZLE_RHS_VALUE_INDEX_MB") {
        if let Ok(mb) = raw.trim().parse::<usize>() {
            return mb.saturating_mul(1024 * 1024);
        }
    }
    // 64 MB. The point of the bounded modes is that they stay small, so the
    // index must not become the thing that breaks that promise; the collapse
    // onto distinct values is steep enough that this covers the positions that
    // matter (5 MB serves 64 million solutions at L=9).
    64 * 1024 * 1024
}

/// A memory-cheap stand-in for [`RhsTable`] used when the full byte cache cannot
/// fit the memory budget — exactly the situation at "extremely large" lengths,
/// where the top-N / streaming modes must still run.
///
/// [`RhsTable`] stores every right-hand side as a byte string (`O(solutions)`
/// memory, which blows the cgroup at scale). The RHS *grammar*, however, is
/// independent of the left-hand side and depends only on a tiny, finite state:
/// the position, the previous character, the floor context, the bracket stack,
/// and the running operand value (itself capped at `MAX_OPERAND_VALUE = 30`).
/// So the set of valid RHS expressions forms a small DFA. We enumerate that DFA
/// **once** per main-operator position and replay it for every LHS, skipping
/// the per-node grammar re-checks the uncached search would otherwise repeat
/// for each LHS.
///
/// Nodes are deduplicated by grammar state via a memo, so the automaton is the
/// minimal state graph: its size grows only *linearly* with the RHS length — a
/// few hundred MB at L=12, comfortably under the 8 GB cap at very large lengths.
/// Replay emits the same bytes, in the same DFS order, as the uncached search,
/// so it is bit-for-bit equivalent.
struct RhsDfa {
    rhs_len: usize,
    nodes: Vec<DfaNode>,
    /// Index of the start state. Nodes are appended in post-order (children
    /// before parents), so the root is *not* node 0 — it is whatever index the
    /// top-level `build_dfa_node` call returned.
    root_idx: u32,
    total_leaves: u64,
}

/// One DFA node: the valid next characters (in the same order
/// `fill_candidate_chars` emits them) and, for each, either the child node
/// index or [`TERMINAL`] (this character completes a full RHS expression).
struct DfaNode {
    chars: Vec<u8>,
    children: Vec<u32>,
    /// Every character this node's subtree can still emit, as a charset
    /// bitmask (this node's own outgoing characters plus, transitively, its
    /// children's). Lets a score-bounded sink rule out a subtree from what the
    /// *grammar* can produce rather than from the charset at large.
    reachable: u32,
}

/// Aggregate answer to "how many right-hand sides satisfy `lhs > rhs`, and
/// which characters do they contain?" — without touching a single right-hand
/// side.
///
/// Character statistics never need the RHS strings themselves, only counts.
/// Right-hand sides collapse hard onto their integer values (at L=9, 1.9M
/// expressions share 26k distinct values), so storing one cumulative character
/// histogram per *distinct value*, ordered by value, answers any `lhs > rhs`
/// query with a binary search plus a `CHARSET_LEN` read. That replaces a walk
/// over every (LHS, RHS) pair — `O(LHS x RHS)` — with `O(LHS log RHS)`, and the
/// collapse factor *grows* with length (32x at L=6, 188x at L=9), so the win
/// widens exactly where it is needed.
///
/// Memory is bounded by the number of distinct values, not the solution count:
/// ~5 MB at L=9 against a solution set of 64 million.
struct RhsValueIndex {
    /// Distinct RHS values, ascending.
    values: Vec<i64>,
    /// `cum_counts[i]` = number of right-hand sides with value < `values[i]`.
    cum_counts: Vec<u64>,
    /// Flattened `CHARSET_LEN`-lane prefix sums: `cum_chars[i * CHARSET_LEN + c]`
    /// = how many right-hand sides with value < `values[i]` contain character
    /// `c`. One extra row past the end holds the totals.
    cum_chars: Vec<u64>,
}

impl RhsValueIndex {
    /// Resident bytes, for budgeting.
    fn mem_bytes(&self) -> usize {
        self.values.len() * std::mem::size_of::<i64>()
            + self.cum_counts.len() * std::mem::size_of::<u64>()
            + self.cum_chars.len() * std::mem::size_of::<u64>()
    }

    /// Aggregate over every right-hand side with `value < lhs_value`: the count
    /// and the per-character totals.
    #[inline]
    fn query(&self, lhs_value: i64) -> (u64, [u64; CHARSET_LEN]) {
        // Number of distinct values strictly below `lhs_value`.
        let row = self.values.partition_point(|&v| v < lhs_value);
        let count = self.cum_counts[row];
        let mut chars = [0u64; CHARSET_LEN];
        let base = row * CHARSET_LEN;
        chars.copy_from_slice(&self.cum_chars[base..base + CHARSET_LEN]);
        (count, chars)
    }
}

/// Marker child index meaning "this edge completes a valid RHS expression".
const TERMINAL: u32 = u32::MAX;

impl RhsDfa {
    /// Emit every solution formed by this LHS value and the enumerated RHS
    /// grammar, in the same order the recursive search would have produced
    /// them. Mirrors [`RhsTable::replay`] but regenerates the RHS bytes on the
    /// fly from the DFA instead of copying a stored list.
    #[inline]
    fn replay<S: SolutionSink>(
        &self,
        lhs_value: i64,
        expr: &mut [u8],
        start: usize,
        sink: &mut S,
        searched_count: &mut u64,
    ) {
        *searched_count += self.total_leaves;
        if self.total_leaves == 0 {
            return;
        }
        // The whole subtree is charged to `searched_count` up front, so any
        // pruning inside `walk` changes only the work done, never the reported
        // statistics.
        let present = unique_char_mask(&expr[..start]);
        self.walk(self.root_idx, 0, expr, start, lhs_value, sink, present);
    }

    #[allow(clippy::too_many_arguments)]
    fn walk<S: SolutionSink>(
        &self,
        node_idx: u32,
        depth: usize,
        expr: &mut [u8],
        start: usize,
        lhs_value: i64,
        sink: &mut S,
        present: u32,
    ) {
        let node = &self.nodes[node_idx as usize];
        let rhs_len = self.rhs_len;
        // Ask once per node whether anything this subtree can still produce is
        // worth having. `S::PRUNES` is a compile-time constant, so for sinks
        // that keep everything the whole block — and the `present` bookkeeping
        // below — is eliminated by the optimizer.
        if S::PRUNES && !sink.may_accept_masked(present, node.reachable, rhs_len - depth) {
            return;
        }
        for i in 0..node.chars.len() {
            let ch = node.chars[i];
            let child = node.children[i];
            expr[start + depth] = ch;
            if child == TERMINAL {
                let right_side = &expr[start..start + rhs_len];
                if let Some(rv) = evaluate_expression_solver_bytes(right_side) {
                    if is_integer(rv) {
                        let rhs_value = rv as i64;
                        if lhs_value > rhs_value {
                            sink.accept_valued(expr, rhs_value);
                        }
                    }
                }
            } else {
                let next_present = if S::PRUNES {
                    present | (1u32 << idx_of(ch))
                } else {
                    present
                };
                self.walk(child, depth + 1, expr, start, lhs_value, sink, next_present);
            }
        }
    }

    /// Build the aggregate [`RhsValueIndex`] by walking this automaton once.
    ///
    /// One traversal of the whole right-hand-side set — the same work a
    /// *single* left-hand side would cost — buys `O(log)` aggregate queries for
    /// every left-hand side thereafter. Returns `None` if the index would
    /// exceed `budget_bytes`.
    fn build_value_index(&self, budget_bytes: usize) -> Option<RhsValueIndex> {
        // Per distinct value: how many right-hand sides have it, and how many
        // of those contain each character. One map, so the transient build
        // footprint is the same order as the finished index.
        let mut acc: HashMap<i64, (u64, [u64; CHARSET_LEN])> = HashMap::new();
        let mut buf = vec![NO_CHAR; self.rhs_len];

        // Cap the distinct-value count so the *build* stays inside the budget,
        // not just the finished index. The accumulator is a hash map, which
        // carries hashes, control bytes and load-factor slack on top of each
        // entry, so it is charged several times the packed cost — otherwise a
        // position with a huge value set balloons right up to the moment it is
        // rejected, and the peak lands well above the budget it was meant to
        // respect.
        let packed_per_value = std::mem::size_of::<i64>()
            + std::mem::size_of::<u64>()
            + CHARSET_LEN * std::mem::size_of::<u64>();
        let max_values = budget_bytes / (packed_per_value * VALUE_INDEX_BUILD_OVERHEAD).max(1);

        self.collect_values(
            self.root_idx,
            0,
            &mut buf,
            &mut |value, mask, acc: &mut HashMap<i64, (u64, [u64; CHARSET_LEN])>| {
                let entry = acc.entry(value).or_insert((0, [0u64; CHARSET_LEN]));
                entry.0 += 1;
                let mut m = mask;
                while m != 0 {
                    let i = m.trailing_zeros() as usize;
                    entry.1[i] += 1;
                    m &= m - 1;
                }
            },
            &mut acc,
            max_values,
        )?;

        let mut values: Vec<i64> = acc.keys().copied().collect();
        values.sort_unstable();

        // Prefix sums, so a query is "everything strictly below this value".
        let rows = values.len() + 1;
        let mut cum_counts = vec![0u64; rows];
        let mut cum_chars = vec![0u64; rows * CHARSET_LEN];
        let mut running = 0u64;
        let mut running_chars = [0u64; CHARSET_LEN];
        for (row, &v) in values.iter().enumerate() {
            cum_counts[row] = running;
            cum_chars[row * CHARSET_LEN..(row + 1) * CHARSET_LEN].copy_from_slice(&running_chars);
            let (count, lanes) = &acc[&v];
            running += count;
            for i in 0..CHARSET_LEN {
                running_chars[i] += lanes[i];
            }
        }
        cum_counts[rows - 1] = running;
        cum_chars[(rows - 1) * CHARSET_LEN..rows * CHARSET_LEN].copy_from_slice(&running_chars);

        let index = RhsValueIndex {
            values,
            cum_counts,
            cum_chars,
        };
        (index.mem_bytes() <= budget_bytes).then_some(index)
    }

    /// Enumerate every right-hand side once, folding `(value, char mask)` into
    /// `acc`. Returns `None` as soon as `acc` exceeds `max_values` distinct
    /// values — the index would not fit its budget, and bailing there keeps the
    /// *build* bounded too, not just the finished index.
    fn collect_values(
        &self,
        node_idx: u32,
        depth: usize,
        buf: &mut [u8],
        emit: &mut impl FnMut(i64, u32, &mut HashMap<i64, (u64, [u64; CHARSET_LEN])>),
        acc: &mut HashMap<i64, (u64, [u64; CHARSET_LEN])>,
        max_values: usize,
    ) -> Option<()> {
        let node = &self.nodes[node_idx as usize];
        for i in 0..node.chars.len() {
            let ch = node.chars[i];
            let child = node.children[i];
            buf[depth] = ch;
            if child == TERMINAL {
                if let Some(rv) = evaluate_expression_solver_bytes(&buf[..self.rhs_len]) {
                    if is_integer(rv) {
                        emit(rv as i64, unique_char_mask(&buf[..self.rhs_len]), acc);
                        if acc.len() > max_values {
                            return None;
                        }
                    }
                }
            } else {
                self.collect_values(child, depth + 1, buf, emit, acc, max_values)?;
            }
        }
        Some(())
    }
}

/// Pack the RHS grammar state into a single `u64` key. The grammar depends only
/// on these components (all finite and small, since the operand value is capped
/// at `MAX_OPERAND_VALUE`); for unconstrained puzzles it does not depend on
/// `char_counts`, so this key fully determines the candidate set.
#[inline]
#[allow(clippy::too_many_arguments)]
fn pack_dfa_state(
    abs_index: usize,
    prev_char: Option<u8>,
    floor: FloorContext,
    bracket_stack: &[u8],
    stack_len: usize,
    num_len: u8,
    num_value: i64,
    num_leading_zero: bool,
) -> u64 {
    let prev_code: u8 = match prev_char {
        None => 31,
        Some(b'>') => 30,
        Some(c) => idx_of(c) as u8,
    };
    let mut key = 0u64;
    key |= abs_index as u64 & 0xFF;
    key |= (prev_code as u64 & 0x1F) << 8;
    key |= ((floor.in_floor as u64) & 1) << 13;
    key |= ((floor.has_slash_in_current_floor as u64) & 1) << 14;
    // Bracket stack: 5 levels x 2 bits (0 = empty, 1 = '(', 2 = '[').
    // `take(stack_len.min(5))` keeps the semantics of the original
    // `if i < stack_len { bracket_stack[i] } else { 0 }` — positions past the
    // live stack are encoded as 0, not as whatever `NO_CHAR` byte lingers in
    // the backing buffer.
    for (i, slot) in bracket_stack.iter().take(stack_len.min(5)).enumerate() {
        let code = match *slot {
            b'(' => 1u64,
            b'[' => 2u64,
            _ => 0u64,
        };
        key |= code << (15 + 2 * i);
    }
    key |= ((stack_len as u64) & 0x7) << 25;
    key |= ((num_len as u64) & 0x3) << 28;
    key |= ((num_value as u64) & 0x1F) << 30;
    key |= ((num_leading_zero as u64) & 1) << 35;
    key
}

#[derive(Debug, Clone)]
struct PreparedKnowledge {
    fixed_chars: Vec<u8>,
    cannot_be_at_masks: Vec<u32>,
    globally_forbidden_mask: u32,
    min_counts: [u8; CHARSET_LEN],
    exact_counts: [u8; CHARSET_LEN],
    exact_mask: u32,
    constrained_indices: Vec<usize>,
    unconstrained: bool,
}

impl PreparedKnowledge {
    fn new(length: usize, gk: &GlobalKnowledge) -> Self {
        let mut fixed_chars = vec![NO_CHAR; length];
        for (i, fixed) in gk.fixed_chars.iter().enumerate() {
            // A fixed char comes from a `Correct` tile, which is untrusted
            // API/CLI input and may carry a byte outside the Sumzle charset.
            // Such a byte must never reach the search: `idx_of`/`char_mask`
            // would map it to `INVALID_INDEX` (255) and index out of bounds.
            // Drop it (no constraint) to match how the other constraint paths
            // skip unrepresentable characters.
            fixed_chars[i] = fixed
                .filter(|&c| idx_of_char(c).is_some())
                .map(|c| c as u8)
                .unwrap_or(NO_CHAR);
        }

        let mut cannot_be_at_masks = vec![0u32; length];
        for (i, set) in gk.cannot_be_at.iter().enumerate() {
            let mut mask = 0u32;
            for &ch in set {
                if let Some(idx) = idx_of_char(ch) {
                    mask |= 1u32 << idx;
                }
            }
            cannot_be_at_masks[i] = mask;
        }

        let mut globally_forbidden_mask = 0u32;
        for &ch in &gk.globally_forbidden {
            if let Some(idx) = idx_of_char(ch) {
                globally_forbidden_mask |= 1u32 << idx;
            }
        }

        let mut min_counts = [0u8; CHARSET_LEN];
        let mut exact_counts = [0u8; CHARSET_LEN];
        let mut exact_mask = 0u32;
        let mut constrained_mask = 0u32;

        for (&ch, &count) in &gk.must_appear_min_count {
            if let Some(idx) = idx_of_char(ch) {
                min_counts[idx] = count as u8;
                constrained_mask |= 1u32 << idx;
            }
        }

        for (&ch, &count) in &gk.must_appear_exact_count {
            if let Some(idx) = idx_of_char(ch) {
                exact_counts[idx] = count as u8;
                exact_mask |= 1u32 << idx;
                constrained_mask |= 1u32 << idx;
            }
        }

        let mut constrained_indices = Vec::new();
        for idx in 0..CHARSET_LEN {
            if constrained_mask & (1u32 << idx) != 0 {
                constrained_indices.push(idx);
            }
        }

        let unconstrained = fixed_chars.iter().all(|&ch| ch == NO_CHAR)
            && cannot_be_at_masks.iter().all(|&mask| mask == 0)
            && globally_forbidden_mask == 0
            && constrained_indices.is_empty();

        Self {
            fixed_chars,
            cannot_be_at_masks,
            globally_forbidden_mask,
            min_counts,
            exact_counts,
            exact_mask,
            constrained_indices,
            unconstrained,
        }
    }

    #[inline]
    fn is_globally_forbidden(&self, ch: u8) -> bool {
        self.globally_forbidden_mask & char_mask(ch) != 0
    }

    #[inline]
    fn cannot_be_at(&self, index: usize, ch: u8) -> bool {
        self.cannot_be_at_masks[index] & char_mask(ch) != 0
    }

    #[inline]
    fn counts_can_still_succeed(&self, counts: &[u8; CHARSET_LEN], remaining_slots: usize) -> bool {
        for &idx in &self.constrained_indices {
            let current = counts[idx] as usize;
            if self.exact_mask & (1u32 << idx) != 0 {
                let exact = self.exact_counts[idx] as usize;
                if current > exact || current + remaining_slots < exact {
                    return false;
                }
            } else {
                let min = self.min_counts[idx] as usize;
                if current + remaining_slots < min {
                    return false;
                }
            }
        }
        true
    }
}

#[inline]
fn idx_of(ch: u8) -> usize {
    let idx = CHAR_INDEX[ch as usize];
    debug_assert_ne!(idx, INVALID_INDEX, "invalid Sumzle character: {ch}");
    idx as usize
}

#[inline]
fn idx_of_char(ch: char) -> Option<usize> {
    // Returns `None` for any character outside the Sumzle charset. Guess rows
    // come from untrusted API/CLI input and may contain arbitrary characters,
    // so this must never panic: an unrepresentable character simply has no
    // charset index, and callers drop it when building constraints.
    if !ch.is_ascii() {
        return None;
    }
    Some(match ch as u8 {
        b'0' => 0,
        b'1' => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        b'5' => 5,
        b'6' => 6,
        b'7' => 7,
        b'8' => 8,
        b'9' => 9,
        b'+' => 10,
        b'-' => 11,
        b'*' => 12,
        b'/' => 13,
        b'%' => 14,
        b'^' => 15,
        b'=' => 16,
        b'(' => 17,
        b')' => 18,
        b'!' => 19,
        b'[' => 20,
        b']' => 21,
        b'>' => 22,
        b'A' => 23,
        _ => return None,
    })
}

#[inline]
fn char_mask(ch: u8) -> u32 {
    1u32 << idx_of(ch)
}

#[inline]
fn unconstrained_solution_capacity(length: usize) -> usize {
    match length {
        3 => 64,
        4 => 320,
        5 => 6_500,
        6 => 50_000,
        7 => 650_000,
        8 => 8_000_000,
        _ => 0,
    }
}

#[inline]
fn write_i64_decimal(n: i64, out: &mut [u8; 20]) -> usize {
    let mut value = n.unsigned_abs();
    let mut tmp = [0u8; 20];
    let mut idx = tmp.len();

    if value == 0 {
        idx -= 1;
        tmp[idx] = b'0';
    } else {
        while value > 0 {
            idx -= 1;
            tmp[idx] = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }

    let mut len = 0;
    if n < 0 {
        out[0] = b'-';
        len = 1;
    }
    let digits = &tmp[idx..];
    out[len..len + digits.len()].copy_from_slice(digits);
    len + digits.len()
}

#[inline]
const fn is_digit_b(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

#[inline]
const fn is_binary_operator_b(c: u8) -> bool {
    matches!(c, b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'A')
}

#[inline]
const fn is_unary_post_operator_b(c: u8) -> bool {
    c == b'!'
}

#[inline]
const fn is_operator_b(c: u8) -> bool {
    is_binary_operator_b(c) || is_unary_post_operator_b(c)
}

#[inline]
const fn is_open_bracket_b(c: u8) -> bool {
    matches!(c, b'(' | b'[')
}

#[inline]
const fn is_close_bracket_b(c: u8) -> bool {
    matches!(c, b')' | b']')
}

#[inline]
const fn is_main_operator_b(c: u8) -> bool {
    matches!(c, b'=' | b'>')
}

#[inline]
const fn is_end_char_b(c: u8) -> bool {
    is_digit_b(c) || matches!(c, b')' | b']' | b'!')
}

#[inline]
const fn matches_bracket(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']'))
}

#[inline]
fn update_floor_context(ch: u8, ctx: FloorContext) -> FloorContext {
    match ch {
        b'[' => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: false,
        },
        b']' if ctx.in_floor => FloorContext {
            in_floor: false,
            has_slash_in_current_floor: false,
        },
        b'/' if ctx.in_floor => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: true,
        },
        _ => ctx,
    }
}

#[inline]
fn base_candidates(
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
) -> &'static [u8] {
    if floor_ctx.in_floor {
        if floor_ctx.has_slash_in_current_floor {
            FLOOR_WITH_SLASH
        } else {
            FLOOR_NO_SLASH
        }
    } else if main_op_so_far == Some(b'=') {
        if prev_char == Some(b'=') {
            AFTER_EQ_START
        } else {
            AFTER_EQ
        }
    } else if index == 0 {
        FIRST_POSITION
    } else if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            AFTER_DIGIT
        } else if is_binary_operator_b(pc) || is_open_bracket_b(pc) {
            AFTER_BINARY_OR_OPEN
        } else if is_close_bracket_b(pc) || is_unary_post_operator_b(pc) {
            AFTER_CLOSE_OR_FACTORIAL
        } else if is_main_operator_b(pc) {
            AFTER_BINARY_OR_OPEN
        } else {
            DEFAULT_ORDER
        }
    } else {
        DEFAULT_ORDER
    }
}

#[inline]
fn push_filtered(
    slice: &[u8],
    index: usize,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let mut len = 0;
    for &ch in slice {
        if !prepared.is_globally_forbidden(ch) && !prepared.cannot_be_at(index, ch) {
            out[len] = ch;
            len += 1;
        }
    }
    len
}

#[inline]
fn push_filtered_end_chars(
    slice: &[u8],
    index: usize,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let mut len = 0;
    for &ch in slice {
        if is_end_char_b(ch)
            && !prepared.is_globally_forbidden(ch)
            && !prepared.cannot_be_at(index, ch)
        {
            out[len] = ch;
            len += 1;
        }
    }
    len
}

/// Get optimized character order for a given position and context.
#[inline]
fn fill_candidate_chars(
    index: usize,
    prev_char: Option<u8>,
    length: usize,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let fixed = prepared.fixed_chars[index];
    if fixed != NO_CHAR {
        if prepared.is_globally_forbidden(fixed) || prepared.cannot_be_at(index, fixed) {
            return 0;
        }
        out[0] = fixed;
        return 1;
    }

    let ordered = base_candidates(index, prev_char, main_op_so_far, floor_ctx);

    if index == length - 1 && !floor_ctx.in_floor {
        let filtered_len = push_filtered_end_chars(ordered, index, prepared, out);
        if filtered_len > 0 {
            return filtered_len;
        }
        if prev_char.is_some() {
            return push_filtered(END_CHARS, index, prepared, out);
        }
        if index == 0 && length == 1 {
            return push_filtered(LENGTH_ONE_DIGITS, index, prepared, out);
        }
    }

    push_filtered(ordered, index, prepared, out)
}

/// Check if a character can be placed at a given position
#[allow(clippy::too_many_arguments)]
fn can_place_char(
    ch: u8,
    ch_idx: usize,
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    char_counts: &[u8; CHARSET_LEN],
    floor_ctx: FloorContext,
    bracket_stack: &[u8],
    stack_len: usize,
    prepared: &PreparedKnowledge,
    length: usize,
    current_num_len: u8,
    current_num_value: i64,
    current_num_leading_zero: bool,
) -> bool {
    // Candidates are pre-filtered for fixed-position and cannot-be-at constraints.
    // Only count-dependent constraints remain here.

    // Exact count constraint
    if prepared.exact_mask & (1u32 << ch_idx) != 0
        && char_counts[ch_idx] >= prepared.exact_counts[ch_idx]
    {
        return false;
    }

    // Floor context constraints
    if floor_ctx.in_floor {
        if ch == b'[' || ch == b'(' || ch == b'A' || ch == b'!' {
            return false;
        }
        if is_operator_b(ch) && ch != b'/' {
            return false;
        }
        if is_main_operator_b(ch) {
            return false;
        }

        if ch == b'/' {
            if floor_ctx.has_slash_in_current_floor {
                return false;
            }
            if !prev_char.is_some_and(is_digit_b) || index == 0 {
                return false;
            }
        } else if ch == b']' {
            if !prev_char.is_some_and(is_digit_b) {
                return false;
            }
            if !floor_ctx.has_slash_in_current_floor {
                return false;
            }
        } else if !is_digit_b(ch) {
            return false;
        }
    }

    // Floor bracket constraints
    if ch == b'[' && floor_ctx.in_floor {
        return false;
    }
    if ch == b']' && !floor_ctx.in_floor {
        return false;
    }
    if ch == b'[' && index >= length.saturating_sub(3) {
        return false;
    }

    // Leading zero check and operand value check.  Left-side operands are capped;
    // after '=' the RHS is a simple number, so only its leading-zero rule matters.
    if is_digit_b(ch) {
        let digit = (ch - b'0') as i64;
        let continuing_number = prev_char.is_some_and(is_digit_b);
        let new_len = if continuing_number {
            current_num_len as usize + 1
        } else {
            1
        };
        let new_value = if continuing_number {
            current_num_value * 10 + digit
        } else {
            digit
        };
        let leading_zero = if continuing_number {
            current_num_leading_zero
        } else {
            ch == b'0'
        };

        if new_len > 1 && leading_zero {
            return false;
        }
        if main_op_so_far != Some(b'=') && new_value > MAX_OPERAND_VALUE {
            return false;
        }
    }

    // First position rules
    if index == 0
        && (is_binary_operator_b(ch)
            || is_close_bracket_b(ch)
            || is_main_operator_b(ch)
            || is_unary_post_operator_b(ch))
    {
        return false;
    }

    // Previous character-based rules
    if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            if is_open_bracket_b(ch) && ch != b'[' {
                return false;
            }
            if ch == b'[' && floor_ctx.in_floor {
                return false;
            }
        } else if is_operator_b(pc) {
            if is_binary_operator_b(ch)
                && !(pc == b'A' && (is_open_bracket_b(ch) || is_digit_b(ch)))
                && !is_unary_post_operator_b(pc)
            {
                return false;
            }
            if is_close_bracket_b(ch) && !is_unary_post_operator_b(pc) {
                return false;
            }
            if is_main_operator_b(ch) && !is_unary_post_operator_b(pc) {
                return false;
            }
            if is_unary_post_operator_b(pc) && (is_digit_b(ch) || is_open_bracket_b(ch)) {
                return false;
            }
        } else if is_open_bracket_b(pc) {
            if pc == b'[' && ch == b'(' {
                return false;
            }
            if is_binary_operator_b(ch) {
                return false;
            }
            if is_close_bracket_b(ch) && !matches_bracket(pc, ch) {
                return false;
            }
            if is_main_operator_b(ch) {
                return false;
            }
            if is_unary_post_operator_b(ch) {
                return false;
            }
        } else if is_close_bracket_b(pc) {
            if is_digit_b(ch) || is_open_bracket_b(ch) {
                return false;
            }
        } else if is_main_operator_b(pc) {
            if pc == b'=' {
                if !is_digit_b(ch) && ch != b'-' {
                    return false;
                }
            } else if is_main_operator_b(ch) || is_close_bracket_b(ch) {
                return false;
            }
        }
    }

    // After main operator =, only digits and minus
    if main_op_so_far == Some(b'=') {
        if !is_digit_b(ch) && ch != b'-' {
            return false;
        }
        if ch == b'-' && prev_char == Some(b'=') && index >= length - 1 {
            return false;
        }
    }

    // Last position rules
    if index == length - 1
        && (is_binary_operator_b(ch) || is_open_bracket_b(ch) || is_main_operator_b(ch))
    {
        return false;
    }

    // Incremental bracket balance check
    let new_stack_len = match ch {
        b'(' | b'[' => stack_len + 1,
        b')' | b']' => {
            if stack_len == 0 {
                return false;
            }
            let last_open = bracket_stack[stack_len - 1];
            if !matches_bracket(last_open, ch) {
                return false;
            }
            stack_len - 1
        }
        _ => stack_len,
    };

    if index == length - 1 && new_stack_len != 0 {
        return false;
    }

    // Main operator rules
    if is_main_operator_b(ch) {
        if main_op_so_far.is_some() {
            return false;
        }
        if index == 0 || index >= length - 1 {
            return false;
        }
    }

    // Permutation A rules
    if ch == b'A' && !prev_char.is_some_and(|pc| is_digit_b(pc) || is_close_bracket_b(pc)) {
        return false;
    }
    if prev_char == Some(b'A') && !is_digit_b(ch) && !is_open_bracket_b(ch) {
        return false;
    }

    // Factorial ! rules
    if ch == b'!' {
        if prev_char.is_none() {
            return false;
        }
        if let Some(pc) = prev_char {
            if !is_digit_b(pc) && pc != b')' {
                return false;
            }
        }
    }

    true
}

#[allow(clippy::too_many_arguments)]
fn complete_eq_rhs<S: SolutionSink>(
    length: usize,
    main_op_index: usize,
    expr: &mut [u8],
    char_counts: &mut [u8; CHARSET_LEN],
    prepared: &PreparedKnowledge,
    rhs: &[u8],
    sink: &mut S,
    searched_count: &mut u64,
) {
    debug_assert_eq!(rhs.len(), length - main_op_index - 1);

    let mut filled = 0usize;
    let mut valid = true;
    for (offset, &ch) in rhs.iter().enumerate() {
        let pos = main_op_index + 1 + offset;
        if prepared.is_globally_forbidden(ch)
            || prepared.cannot_be_at(pos, ch)
            || (prepared.fixed_chars[pos] != NO_CHAR && prepared.fixed_chars[pos] != ch)
        {
            valid = false;
            break;
        }

        let ch_idx = idx_of(ch);
        if prepared.exact_mask & (1u32 << ch_idx) != 0
            && char_counts[ch_idx] >= prepared.exact_counts[ch_idx]
        {
            valid = false;
            break;
        }

        expr[pos] = ch;
        char_counts[ch_idx] += 1;
        filled += 1;
    }

    if valid && prepared.counts_can_still_succeed(char_counts, 0) {
        *searched_count += 1;
        sink.accept(expr);
    }

    for &ch in &rhs[..filled] {
        char_counts[idx_of(ch)] -= 1;
    }
    for b in expr.iter_mut().take(length).skip(main_op_index + 1) {
        *b = NO_CHAR;
    }
}

/// A captured search node at a fixed prefix depth, used to distribute work
/// across threads at a finer granularity than the ~11 top-level branches.
/// It records everything needed to resume `recursive_search` from `prefix.len()`.
#[derive(Clone)]
pub struct Branch {
    prefix: Vec<u8>,
    main_op: Option<u8>,
    main_op_index: usize,
    main_lhs_value: Option<i64>,
    floor_ctx: FloorContext,
    bracket_stack: Vec<u8>,
    num_len: u8,
    num_value: i64,
    num_leading_zero: bool,
}

/// The main solver struct
pub struct Solver {
    pub length: usize,
    pub gk: GlobalKnowledge,
    prepared: PreparedKnowledge,
    /// Cached right-hand-side enumerations for `>` equations, indexed by the
    /// main operator's position. `None` where caching does not apply (position
    /// can't hold `>`, or the table exceeded its memory budget).
    ///
    /// Empty when the puzzle carries count-based constraints, which make the
    /// RHS subtree depend on the LHS's character usage — the one case where the
    /// "same subtree for every LHS" invariant does not hold.
    rhs_tables: Vec<Option<RhsTable>>,
    /// Memory-cheap RHS grammar DFA (see [`RhsDfa`]), used where the byte cache
    /// was skipped because it would not fit — i.e. at extremely large lengths.
    /// Mirrors `rhs_tables` in indexing; the two are mutually exclusive per
    /// position (a position uses the byte cache when it fits, else the DFA).
    rhs_dfas: Vec<Option<RhsDfa>>,
    /// Aggregate `lhs > rhs` index (see [`RhsValueIndex`]), indexed like
    /// `rhs_dfas`. Lets sinks that need only counts absorb a whole right-hand
    /// side subtree with one lookup instead of enumerating it.
    ///
    /// Built lazily on first use: it costs a full traversal of the
    /// right-hand-side set, which would be pure waste for a solve that emits
    /// or ranks solutions (they cannot use aggregates). `OnceLock` makes the
    /// first counting solve pay for it and every later one free, while keeping
    /// `Solver` shareable across threads.
    rhs_value_indexes: std::sync::OnceLock<Vec<Option<RhsValueIndex>>>,
}

impl Solver {
    pub fn new(length: usize, gk: GlobalKnowledge) -> Self {
        let prepared = PreparedKnowledge::new(length, &gk);
        let mut solver = Self {
            length,
            gk,
            prepared,
            rhs_tables: Vec::new(),
            rhs_dfas: Vec::new(),
            rhs_value_indexes: std::sync::OnceLock::new(),
        };
        solver.rhs_tables = solver.build_rhs_tables();
        solver.rhs_dfas = solver.build_rhs_dfas();
        solver
    }

    /// The aggregate indexes, built on first use (see
    /// [`rhs_value_indexes`](Self::rhs_value_indexes)).
    #[inline]
    fn value_indexes(&self) -> &[Option<RhsValueIndex>] {
        self.rhs_value_indexes
            .get_or_init(|| self.build_rhs_value_indexes())
    }

    /// Per-position `(rhs_len, entries, bytes_used)` for the cached RHS tables.
    /// Diagnostic aid for sizing the memory budget; empty entries are omitted.
    pub fn rhs_table_stats(&self) -> Vec<(usize, usize, usize, usize)> {
        self.rhs_tables
            .iter()
            .enumerate()
            .filter_map(|(k, t)| t.as_ref().map(|t| (k, t.rhs_len, t.len(), t.mem_bytes())))
            .collect()
    }

    /// Exact number of complete expressions the right-hand-side subtree of a
    /// `>` at index `k` reaches — i.e. what the search would add to its
    /// "searched" statistic by walking it.
    ///
    /// Only available where the subtree was enumerated up front (byte cache or
    /// grammar DFA). `None` elsewhere, which forces the caller to walk the
    /// subtree normally rather than guess at the count.
    #[inline]
    fn rhs_subtree_leaves(&self, k: usize) -> Option<u64> {
        if let Some(Some(t)) = self.rhs_tables.get(k) {
            return Some(t.total_leaves);
        }
        if let Some(Some(d)) = self.rhs_dfas.get(k) {
            return Some(d.total_leaves);
        }
        None
    }

    /// Build the aggregate `lhs > rhs` index (see [`RhsValueIndex`]) for every
    /// position that has a grammar DFA to enumerate it from.
    ///
    /// Each index costs one traversal of its right-hand-side set — the same
    /// work a single left-hand side would have cost — and then serves every
    /// left-hand side with a binary search.
    ///
    /// Positions are indexed largest-first so the budget goes where the most
    /// work is saved, and one at a time: the build accumulator is transient but
    /// large, and running several at once would multiply the peak by the thread
    /// count — the opposite of what a memory-bounded mode promises. The
    /// traversal itself is the expensive part and is already parallel inside
    /// [`RhsDfa::build_value_index`].
    fn build_rhs_value_indexes(&self) -> Vec<Option<RhsValueIndex>> {
        if self.rhs_dfas.is_empty() {
            return Vec::new();
        }
        let mut remaining = value_index_budget_bytes();
        if remaining == 0 {
            return Vec::new();
        }

        let mut order: Vec<usize> = (0..self.rhs_dfas.len())
            .filter(|&k| {
                self.rhs_dfas[k]
                    .as_ref()
                    // Building the index walks the right-hand-side set once,
                    // which only pays off when many left-hand sides query it.
                    // Past this size the traversal costs more than it saves and
                    // the position keeps the ordinary replay path.
                    .is_some_and(|d| d.total_leaves <= VALUE_INDEX_MAX_LEAVES)
            })
            .collect();
        order.sort_by_key(|&k| std::cmp::Reverse(self.rhs_dfas[k].as_ref().unwrap().total_leaves));

        let mut indexes: Vec<Option<RhsValueIndex>> =
            (0..self.rhs_dfas.len()).map(|_| None).collect();
        for k in order {
            if remaining == 0 {
                break;
            }
            let dfa = self.rhs_dfas[k].as_ref().unwrap();
            if let Some(index) = dfa.build_value_index(remaining) {
                remaining -= index.mem_bytes().min(remaining);
                indexes[k] = Some(index);
            }
        }
        indexes
    }

    /// Per-position `(k, distinct_values, bytes)` for the aggregate value
    /// indexes. Diagnostic aid for sizing their budget.
    pub fn rhs_value_index_stats(&self) -> Vec<(usize, usize, usize)> {
        self.value_indexes()
            .iter()
            .enumerate()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k, v.values.len(), v.mem_bytes())))
            .collect()
    }

    /// Per-position `(rhs_len, nodes, root_idx, total_leaves)` for the grammar
    /// DFAs. Diagnostic aid for sizing [`DFA_NODE_BUDGET`].
    pub fn rhs_dfa_stats(&self) -> Vec<(usize, usize, usize, u64)> {
        self.rhs_dfas
            .iter()
            .enumerate()
            .filter_map(|(k, d)| {
                d.as_ref()
                    .map(|d| (k, d.rhs_len, d.nodes.len(), d.total_leaves))
            })
            .collect()
    }

    /// Whether the cached-RHS optimization may be used for this puzzle.
    ///
    /// The cache replays one enumeration of the RHS subtree for every LHS, so
    /// it is only valid when that subtree does not depend on the LHS.
    ///
    /// *Count* constraints (`must_appear_min_count` / `must_appear_exact_count`)
    /// break this: whether an RHS character is placeable depends on how many
    /// times the LHS already used it. Those puzzles keep the original path.
    ///
    /// *Positional* constraints (fixed chars, cannot-be-at, globally forbidden)
    /// are safe — they are indexed by absolute position, which is fixed once the
    /// operator position is, and the capture run applies them exactly as the
    /// normal search would.
    #[inline]
    fn rhs_cache_applicable(&self) -> bool {
        self.prepared.constrained_indices.is_empty()
    }

    /// Enumerate, once per main-operator position, the complete set of
    /// right-hand sides a `>` equation can take, along with each one's value.
    ///
    /// Built eagerly in `new` so every thread of a parallel solve shares one
    /// immutable copy through the `&Solver` it already holds.
    fn build_rhs_tables(&self) -> Vec<Option<RhsTable>> {
        if !self.rhs_cache_applicable() {
            return Vec::new();
        }

        let mut remaining = rhs_cache_budget_bytes();
        if remaining == 0 {
            return Vec::new();
        }

        // `>` must have at least one character on each side.
        let mut tables: Vec<Option<RhsTable>> = (0..self.length).map(|_| None).collect();

        // Build the *longest* right-hand side first, then work down.
        //
        // The saving from caching position `k` is proportional to the work its
        // subtree represents — (number of left-hand sides) × (size of the RHS
        // subtree) — and that product is overwhelmingly dominated by the
        // smallest `k`, where the RHS spans nearly the whole expression. It is
        // not close: at L=10, caching every position *except* k=1 runs in
        // 46.7s, versus 48.2s with no cache at all and 7.5s with k=1 included.
        // The one giant table is the optimization; the rest are rounding error.
        //
        // So the budget must go to the biggest table first. Spending it on the
        // cheap ones would leave nothing for the only one that matters.
        #[allow(clippy::needless_range_loop)]
        for k in 1..self.length.saturating_sub(1) {
            // Enumerating a table that cannot possibly fit is pure waste — at
            // L=12 the k=1 attempt burned 160 s of a 184 s build before hitting
            // the budget and throwing everything away. The entry count depends
            // only on the RHS length, so predict it and skip.
            if !rhs_table_can_fit(self.length - k - 1, remaining) {
                continue;
            }
            // Skip positions where `>` itself cannot be placed; there is no RHS
            // subtree to cache.
            if self.prepared.is_globally_forbidden(b'>')
                || self.prepared.cannot_be_at(k, b'>')
                || (self.prepared.fixed_chars[k] != NO_CHAR && self.prepared.fixed_chars[k] != b'>')
            {
                continue;
            }
            if let Some(table) = self.build_rhs_table(k, remaining) {
                remaining -= table.mem_bytes().min(remaining);
                tables[k] = Some(table);
            }
        }

        tables
    }

    /// Build the memory-cheap RHS grammar DFA (see [`RhsDfa`]) for every
    /// main-operator position where the full byte cache was skipped — i.e. the
    /// positions too large to fit in the byte-cache budget, which is precisely
    /// where the uncached search would otherwise re-walk the RHS subtree for
    /// every left-hand side. Built largest-RHS-first within [`DFA_NODE_BUDGET`].
    fn build_rhs_dfas(&self) -> Vec<Option<RhsDfa>> {
        if !self.rhs_cache_applicable() {
            return Vec::new();
        }
        let mut remaining = dfa_node_budget();
        if remaining == 0 {
            return Vec::new();
        }
        let mut dfas: Vec<Option<RhsDfa>> = (0..self.length).map(|_| None).collect();

        #[allow(clippy::needless_range_loop)]
        for k in 1..self.length.saturating_sub(1) {
            // Skip positions already covered by the byte cache. `rhs_tables`
            // may be empty when the byte-cache budget is zero (every position
            // falls back to the DFA), so index it bounds-safely.
            if self.rhs_tables.get(k).is_some_and(|t| t.is_some()) {
                continue;
            }
            let rhs_len = self.length - k - 1;
            if rhs_len == 0 || rhs_len > DFA_MAX_RHS_LEN {
                continue;
            }
            // Skip positions where `>` itself cannot be placed.
            if self.prepared.is_globally_forbidden(b'>')
                || self.prepared.cannot_be_at(k, b'>')
                || (self.prepared.fixed_chars[k] != NO_CHAR && self.prepared.fixed_chars[k] != b'>')
            {
                continue;
            }
            // Cheap upper bound on the deduplicated node count; skip rather than
            // build a DFA that would blow the budget.
            if (rhs_len as u64 + 1) * DFA_STATE_FACTOR > remaining {
                continue;
            }
            if let Some(dfa) = self.build_rhs_dfa(k) {
                let n = dfa.nodes.len() as u64;
                if n <= remaining {
                    remaining -= n;
                    dfas[k] = Some(dfa);
                }
            }
        }

        dfas
    }

    /// Enumerate the RHS grammar DFA for `>` at index `k`.
    ///
    /// Calls the *same* `fill_candidate_chars` + `can_place_char` + floor
    /// `min_needed` prune the uncached search uses, so the replayed set and its
    /// order are identical. Nodes are deduplicated by packed grammar state,
    /// bounding memory to the finite state space.
    fn build_rhs_dfa(&self, k: usize) -> Option<RhsDfa> {
        let length = self.length;
        let start = k + 1;
        let rhs_len = length - start;
        if rhs_len == 0 {
            return None;
        }
        let mut nodes: Vec<DfaNode> = Vec::new();
        let mut memo: HashMap<u64, u32> = HashMap::new();
        let mut bracket_buf = vec![NO_CHAR; length];
        let root_key = pack_dfa_state(
            start,
            Some(b'>'),
            FloorContext::new(),
            &bracket_buf,
            0,
            0,
            0,
            false,
        );
        let root_idx = self.build_dfa_node(
            root_key,
            start,
            Some(b'>'),
            FloorContext::new(),
            &mut bracket_buf,
            0,
            0u8,
            0i64,
            false,
            &mut nodes,
            &mut memo,
            length,
        );
        if nodes.is_empty() {
            return None;
        }

        // Count the *complete RHS strings* (root-to-terminal paths). Because
        // nodes are deduplicated, the number of terminal edges in the graph is
        // not the number of expressions — the same sub-automaton is shared by
        // many prefixes — so the path count must be accumulated explicitly.
        // Every child was pushed before its parent, so a single forward pass
        // over `nodes` visits children first.
        let mut leaf_counts: Vec<u64> = vec![0; nodes.len()];
        for i in 0..nodes.len() {
            let mut sum = 0u64;
            for &child in &nodes[i].children {
                sum += if child == TERMINAL {
                    1
                } else {
                    leaf_counts[child as usize]
                };
            }
            leaf_counts[i] = sum;
        }
        let total_leaves = leaf_counts[root_idx as usize];
        if total_leaves == 0 {
            return None;
        }

        // Drop edges into sub-automata that can never complete an expression.
        // They emit nothing, so replay stays bit-identical while skipping the
        // dead branches the uncached search would have walked into.
        for node in &mut nodes {
            if node
                .children
                .iter()
                .any(|&c| c != TERMINAL && leaf_counts[c as usize] == 0)
            {
                let mut chars = Vec::with_capacity(node.chars.len());
                let mut children = Vec::with_capacity(node.children.len());
                for (&ch, &child) in node.chars.iter().zip(node.children.iter()) {
                    if child != TERMINAL && leaf_counts[child as usize] == 0 {
                        continue;
                    }
                    chars.push(ch);
                    children.push(child);
                }
                node.chars = chars;
                node.children = children;
            }
        }

        // Propagate the reachable-character sets. Children are pushed before
        // their parents, so one forward pass suffices — and it must come after
        // dead-branch pruning, or characters that only occur on branches that
        // can never complete would loosen every bound above them.
        for i in 0..nodes.len() {
            let mut mask = 0u32;
            for (&ch, &child) in nodes[i].chars.iter().zip(nodes[i].children.iter()) {
                mask |= 1u32 << idx_of(ch);
                if child != TERMINAL {
                    mask |= nodes[child as usize].reachable;
                }
            }
            nodes[i].reachable = mask;
        }

        Some(RhsDfa {
            rhs_len,
            nodes,
            root_idx,
            total_leaves,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_dfa_node(
        &self,
        key: u64,
        abs_index: usize,
        prev_char: Option<u8>,
        floor: FloorContext,
        bracket_stack: &mut [u8],
        stack_len: usize,
        num_len: u8,
        num_value: i64,
        num_leading_zero: bool,
        nodes: &mut Vec<DfaNode>,
        memo: &mut HashMap<u64, u32>,
        length: usize,
    ) -> u32 {
        if let Some(&idx) = memo.get(&key) {
            return idx;
        }
        // Floor-context `min_needed` prune — the only prune the uncached RHS
        // walk applies here (count constraints are absent for the unconstrained
        // puzzles this DFA is built for).
        let remaining = length - abs_index;
        if floor.in_floor {
            let min_needed = if floor.has_slash_in_current_floor {
                1
            } else {
                3
            };
            if remaining < min_needed {
                let idx = nodes.len() as u32;
                nodes.push(DfaNode {
                    chars: Vec::new(),
                    children: Vec::new(),
                    reachable: 0,
                });
                memo.insert(key, idx);
                return idx;
            }
        }

        let mut out = [NO_CHAR; CHARSET_LEN];
        let count = fill_candidate_chars(
            abs_index,
            prev_char,
            length,
            Some(b'>'),
            floor,
            &self.prepared,
            &mut out,
        );
        let mut chars: Vec<u8> = Vec::with_capacity(count.min(CHARSET_LEN));
        let mut children: Vec<u32> = Vec::with_capacity(count.min(CHARSET_LEN));
        let empty_counts = [0u8; CHARSET_LEN];
        for &ch in &out[..count] {
            if !can_place_char(
                ch,
                idx_of(ch),
                abs_index,
                prev_char,
                Some(b'>'),
                &empty_counts,
                floor,
                bracket_stack,
                stack_len,
                &self.prepared,
                length,
                num_len,
                num_value,
                num_leading_zero,
            ) {
                continue;
            }
            chars.push(ch);
            let next_floor = update_floor_context(ch, floor);
            let (next_num_len, next_num_value, next_leading) = if is_digit_b(ch) {
                if prev_char.is_some_and(is_digit_b) {
                    (
                        num_len + 1,
                        num_value * 10 + (ch - b'0') as i64,
                        num_leading_zero,
                    )
                } else {
                    (1u8, (ch - b'0') as i64, ch == b'0')
                }
            } else {
                (0u8, 0i64, false)
            };
            let saved_slot = if matches!(ch, b'(' | b'[') {
                let s = bracket_stack[stack_len];
                bracket_stack[stack_len] = ch;
                s
            } else {
                NO_CHAR
            };
            let next_stack_len = match ch {
                b'(' | b'[' => stack_len + 1,
                b')' | b']' => stack_len - 1,
                _ => stack_len,
            };
            if abs_index + 1 == length {
                children.push(TERMINAL);
            } else {
                let child_key = pack_dfa_state(
                    abs_index + 1,
                    Some(ch),
                    next_floor,
                    &bracket_stack[..next_stack_len],
                    next_stack_len,
                    next_num_len,
                    next_num_value,
                    next_leading,
                );
                let child_idx = self.build_dfa_node(
                    child_key,
                    abs_index + 1,
                    Some(ch),
                    next_floor,
                    bracket_stack,
                    next_stack_len,
                    next_num_len,
                    next_num_value,
                    next_leading,
                    nodes,
                    memo,
                    length,
                );
                children.push(child_idx);
            }
            if matches!(ch, b'(' | b'[') {
                bracket_stack[stack_len] = saved_slot;
            }
        }
        let idx = nodes.len() as u32;
        nodes.push(DfaNode {
            chars,
            children,
            // Filled in by a forward pass once the graph is complete; children
            // are already present here, but dead-branch pruning still has to
            // run first, so computing it now would be wrong.
            reachable: 0,
        });
        memo.insert(key, idx);
        idx
    }

    /// Enumerate the RHS subtree for `>` at index `k`.
    ///
    /// This runs the *same* `recursive_search` the solver would have run, with
    /// the same state it would have had — so the captured set, and its order,
    /// are exactly what the uncached search produces. Only the LHS bytes differ,
    /// and the RHS grammar never reads them (see [`RhsTable`]).
    /// Building a table is itself a full enumeration of the RHS subtree — at
    /// large lengths the single most expensive phase of the whole solve — so it
    /// runs on every core. The subtree is split into prefix branches exactly
    /// like a parallel solve, and because the cut is placed *strictly before*
    /// the final index no expression can complete during partitioning: the
    /// branches therefore tile the DFS sequence in order, and concatenating
    /// their captures reproduces the serial order entry for entry.
    fn build_rhs_table(&self, k: usize, budget_bytes: usize) -> Option<RhsTable> {
        let rhs_len = self.length - k - 1;
        debug_assert!(rhs_len >= 1);

        let budget = AtomicIsize::new(budget_bytes as isize);
        let (branches, collect_searched) = self.collect_rhs_branches(k);
        // Nothing may terminate before the cut, or the branches would no longer
        // cover the whole subtree.
        debug_assert_eq!(collect_searched, 0);

        let outcomes: Vec<Option<(RhsPart, u64)>> = if branches.is_empty() {
            // RHS too short to partition: capture it in one go.
            vec![self.capture_rhs_subtree(k, None, &budget)]
        } else {
            branches
                .par_iter()
                .map(|branch| {
                    // One branch blowing the budget dooms the whole table, so
                    // don't start branches that would only be thrown away.
                    if budget.load(Ordering::Relaxed) <= 0 {
                        return None;
                    }
                    self.capture_rhs_subtree(k, Some(branch), &budget)
                })
                .collect()
        };

        let mut parts: Vec<RhsPart> = Vec::with_capacity(outcomes.len());
        let mut total_leaves = collect_searched;
        let mut min_value = i64::MAX;
        let mut max_value = i64::MIN;
        for outcome in outcomes {
            // Any branch that blew the budget invalidates the whole table.
            let (part, searched) = outcome?;
            total_leaves += searched;
            if !part.values.is_empty() {
                min_value = min_value.min(part.min_value);
                max_value = max_value.max(part.max_value);
                parts.push(part);
            }
        }

        Some(RhsTable {
            rhs_len,
            total_leaves,
            parts,
            min_value,
            max_value,
        })
    }

    /// Partition the RHS subtree of a `>` at index `k` into prefix branches,
    /// deepening the cut until there is plenty of work per core. Returns an
    /// empty branch list when the RHS is too short to split.
    fn collect_rhs_branches(&self, k: usize) -> (Vec<Branch>, u64) {
        // The cut must stay strictly inside the RHS: `k + 1` would just hand
        // back the root, and `self.length` would let expressions complete.
        let max_cut = self.length - 1;
        if k + 2 > max_cut {
            return (Vec::new(), 0);
        }

        let target = rayon::current_num_threads().saturating_mul(16).max(16);
        let mut cut = k + 2;
        let (mut branches, mut searched) = self.collect_rhs_branches_at(k, cut);
        while branches.len() < target && cut < max_cut {
            cut += 1;
            let next = self.collect_rhs_branches_at(k, cut);
            branches = next.0;
            searched = next.1;
        }
        (branches, searched)
    }

    fn collect_rhs_branches_at(&self, k: usize, cut: usize) -> (Vec<Branch>, u64) {
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        expr[k] = b'>';
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
        let mut branches: Vec<Branch> = Vec::new();
        let mut searched: u64 = 0;

        self.recursive_search(
            k + 1,
            &mut expr,
            Some(b'>'),
            Some(b'>'),
            k,
            Some(i64::MAX),
            &mut char_counts,
            FloorContext::new(),
            &mut bracket_stack,
            0,
            &self.prepared,
            0,
            0,
            false,
            &mut NullSink,
            &mut searched,
            cut,
            &mut branches,
        );

        (branches, searched)
    }

    /// Capture one RHS branch (or the whole subtree when `branch` is `None`).
    ///
    /// Returns `None` if the shared budget ran out. `main_lhs_value` is
    /// `i64::MAX` so the terminal `lhs > rhs` test accepts every syntactically
    /// valid, integer-valued RHS; the real comparison happens at replay against
    /// the actual LHS value.
    fn capture_rhs_subtree(
        &self,
        k: usize,
        branch: Option<&Branch>,
        budget: &AtomicIsize,
    ) -> Option<(RhsPart, u64)> {
        let rhs_len = self.length - k - 1;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        expr[k] = b'>';
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
        let mut no_branches: Vec<Branch> = Vec::new();
        let mut searched: u64 = 0;

        let mut capture = RhsCapture {
            start: k + 1,
            rhs_len,
            bytes: Vec::new(),
            values: Vec::new(),
            min_value: i64::MAX,
            max_value: i64::MIN,
            budget,
            reserved: 0,
            overflow: false,
        };

        match branch {
            None => self.recursive_search(
                k + 1,
                &mut expr,
                Some(b'>'),
                Some(b'>'),
                k,
                Some(i64::MAX),
                &mut char_counts,
                FloorContext::new(),
                &mut bracket_stack,
                0,
                &self.prepared,
                0,
                0,
                false,
                &mut capture,
                &mut searched,
                usize::MAX,
                &mut no_branches,
            ),
            Some(branch) => {
                let depth = branch.prefix.len();
                expr[..depth].copy_from_slice(&branch.prefix);
                // Only the RHS characters are real — the LHS slots are still
                // placeholders — and the capture run never counted the `>`
                // itself, so mirror that exactly.
                for &ch in &branch.prefix[k + 1..] {
                    char_counts[idx_of(ch)] += 1;
                }
                let stack_len = branch.bracket_stack.len();
                bracket_stack[..stack_len].copy_from_slice(&branch.bracket_stack);

                self.recursive_search(
                    depth,
                    &mut expr,
                    branch.prefix.last().copied(),
                    branch.main_op,
                    branch.main_op_index,
                    branch.main_lhs_value,
                    &mut char_counts,
                    branch.floor_ctx,
                    &mut bracket_stack,
                    stack_len,
                    &self.prepared,
                    branch.num_len,
                    branch.num_value,
                    branch.num_leading_zero,
                    &mut capture,
                    &mut searched,
                    usize::MAX,
                    &mut no_branches,
                );
            }
        }

        capture.finish().map(|part| (part, searched))
    }

    /// Solve with single-threaded brute force
    pub fn solve(&self) -> (Vec<String>, u64) {
        let mut results: Vec<String> = if self.prepared.unconstrained {
            Vec::with_capacity(unconstrained_solution_capacity(self.length))
        } else {
            Vec::new()
        };
        let searched_count = self.solve_into(&mut results);
        (results, searched_count)
    }

    /// Run the full single-threaded search, delivering every solution to
    /// `sink`. Returns the number of complete expressions evaluated. This is
    /// the generic engine behind `solve`; alternative sinks stream to disk or
    /// score for top-N without building a `Vec<String>`.
    pub fn solve_into<S: SolutionSink>(&self, sink: &mut S) -> u64 {
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
        let mut no_branches: Vec<Branch> = Vec::new();

        self.recursive_search(
            0,
            &mut expr,
            None,
            None,
            0,
            None,
            &mut char_counts,
            FloorContext::new(),
            &mut bracket_stack,
            0,
            &self.prepared,
            0,
            0,
            false,
            sink,
            &mut searched_count,
            usize::MAX,
            &mut no_branches,
        );

        searched_count
    }

    #[allow(clippy::too_many_arguments)]
    fn recursive_search<S: SolutionSink>(
        &self,
        index: usize,
        expr: &mut [u8],
        prev_char: Option<u8>,
        main_op_so_far: Option<u8>,
        main_op_index: usize,
        main_lhs_value: Option<i64>,
        char_counts: &mut [u8; CHARSET_LEN],
        floor_ctx: FloorContext,
        bracket_stack: &mut [u8],
        stack_len: usize,
        prepared: &PreparedKnowledge,
        current_num_len: u8,
        current_num_value: i64,
        current_num_leading_zero: bool,
        sink: &mut S,
        searched_count: &mut u64,
        // Branch-collection cutoff: when `index == branch_depth`, snapshot the
        // current node into `branches` and stop. Normal solves pass
        // `usize::MAX`, so this is a single never-taken comparison per call.
        branch_depth: usize,
        branches: &mut Vec<Branch>,
    ) {
        if index == branch_depth {
            branches.push(Branch {
                prefix: expr[..index].to_vec(),
                main_op: main_op_so_far,
                main_op_index,
                main_lhs_value,
                floor_ctx,
                bracket_stack: bracket_stack[..stack_len].to_vec(),
                num_len: current_num_len,
                num_value: current_num_value,
                num_leading_zero: current_num_leading_zero,
            });
            return;
        }

        // Start of a `>` right-hand side, with that subtree already enumerated.
        if main_op_so_far == Some(b'>') && index == main_op_index + 1 {
            // Ask the sink whether *any* completion of this left-hand side is
            // still worth producing. For a full top-N heap this discards the
            // whole right-hand side subtree — the single largest unit of work
            // the search ever skips. The expressions still count as searched,
            // so only the run time changes.
            if branch_depth == usize::MAX && !sink.may_accept(&expr[..index], self.length - index) {
                if let Some(leaves) = self.rhs_subtree_leaves(main_op_index) {
                    *searched_count += leaves;
                    for b in expr.iter_mut().take(self.length).skip(index) {
                        *b = NO_CHAR;
                    }
                    return;
                }
            }

            // Aggregate fast path: a sink that only needs counts (character
            // statistics) takes the whole right-hand side subtree as a single
            // lookup instead of walking it once per left-hand side.
            if branch_depth == usize::MAX && sink.wants_aggregate() {
                if let Some(Some(index_tbl)) = self.value_indexes().get(main_op_index) {
                    let lhs_value = main_lhs_value.expect("'>' recorded without an LHS value");
                    let (count, chars) = index_tbl.query(lhs_value);
                    if sink.accept_aggregate(&expr[..index], count, &chars) {
                        *searched_count += self
                            .rhs_subtree_leaves(main_op_index)
                            .expect("a value index implies an enumerated subtree");
                        for b in expr.iter_mut().take(self.length).skip(index) {
                            *b = NO_CHAR;
                        }
                        return;
                    }
                }
            }
            if let Some(Some(table)) = self.rhs_tables.get(main_op_index) {
                if branch_depth == usize::MAX {
                    // Normal search: replay the cached RHS set for this LHS
                    // instead of walking the identical subtree again.
                    let lhs_value = main_lhs_value.expect("'>' recorded without an LHS value");
                    table.replay(lhs_value, expr, index, sink, searched_count);
                    for b in expr.iter_mut().take(self.length).skip(index) {
                        *b = NO_CHAR;
                    }
                } else {
                    // Branch collection: cut the branch here rather than
                    // descending into the RHS. Splitting *inside* the RHS would
                    // bake partial right-hand sides into the branch prefixes,
                    // and a resumed branch that starts mid-RHS cannot use the
                    // table — which is what made deeper partitioning (more
                    // threads) progressively slower. Cutting at the boundary
                    // keeps every branch replay-eligible.
                    branches.push(Branch {
                        prefix: expr[..index].to_vec(),
                        main_op: main_op_so_far,
                        main_op_index,
                        main_lhs_value,
                        floor_ctx,
                        bracket_stack: bracket_stack[..stack_len].to_vec(),
                        num_len: current_num_len,
                        num_value: current_num_value,
                        num_leading_zero: current_num_leading_zero,
                    });
                }
                return;
            }
            // Memory-cheap fallback: replay the RHS *grammar* DFA for this LHS
            // instead of re-walking the identical subtree. Same output order as
            // the uncached descent; only the per-node grammar checks are
            // skipped (they are precomputed into the DFA once).
            if let Some(Some(dfa)) = self.rhs_dfas.get(main_op_index) {
                if branch_depth == usize::MAX {
                    let lhs_value = main_lhs_value.expect("'>' recorded without an LHS value");
                    dfa.replay(lhs_value, expr, index, sink, searched_count);
                    for b in expr.iter_mut().take(self.length).skip(index) {
                        *b = NO_CHAR;
                    }
                } else {
                    branches.push(Branch {
                        prefix: expr[..index].to_vec(),
                        main_op: main_op_so_far,
                        main_op_index,
                        main_lhs_value,
                        floor_ctx,
                        bracket_stack: bracket_stack[..stack_len].to_vec(),
                        num_len: current_num_len,
                        num_value: current_num_value,
                        num_leading_zero: current_num_leading_zero,
                    });
                }
                return;
            }
        }
        let remaining_slots = self.length - index;
        if !prepared.counts_can_still_succeed(char_counts, remaining_slots) {
            return;
        }

        if main_op_so_far.is_none() {
            let min_needed = if floor_ctx.in_floor {
                // Slots still required: close the floor, then the main operator
                // and at least one RHS digit (2). To close the floor we need at
                // least `]` (1) when the denominator digit is already placed; the
                // numerator/slash/denominator may still be pending, but this must
                // stay a *lower* bound or valid completions (e.g. `[7/2]=3`) get
                // pruned. has_slash ⇒ as few as 1 more char (`]`); otherwise at
                // least `/d]` (3).
                2 + if floor_ctx.has_slash_in_current_floor {
                    1
                } else {
                    3
                }
            } else {
                2 + stack_len
                    + usize::from(
                        prev_char
                            .is_none_or(|pc| is_binary_operator_b(pc) || is_open_bracket_b(pc)),
                    )
            };
            if remaining_slots < min_needed {
                return;
            }
        } else if floor_ctx.in_floor {
            // RHS of `>` may contain a floor (e.g. `5>[2/3]`). Same lower-bound
            // reasoning: with a slash already present, as little as `]` (1) may
            // remain; without one, at least `/d]` (3).
            let min_needed = if floor_ctx.has_slash_in_current_floor {
                1
            } else {
                3
            };
            if remaining_slots < min_needed {
                return;
            }
        }

        if index == self.length {
            *searched_count += 1;

            if main_op_so_far.is_none() {
                return;
            }

            // Only `>` terminates here: `=` equations emit their right-hand
            // side directly (see the `=` fast path) and never reach this point
            // as a candidate.
            if main_op_so_far != Some(b'>') {
                return;
            }
            let lhs_value = main_lhs_value.expect("main operator value missing");
            let right_side = &expr[main_op_index + 1..];
            if let Some(rv) = evaluate_expression_solver_bytes(right_side) {
                if is_integer(rv) {
                    let rhs_value = rv as i64;
                    if lhs_value > rhs_value {
                        // Hand the value over so an `RhsCapture` need not
                        // re-evaluate; every other sink drops it.
                        sink.accept_valued(expr, rhs_value);
                    }
                }
            }
            return;
        }

        let mut candidates = [NO_CHAR; CHARSET_LEN];
        let candidate_count = fill_candidate_chars(
            index,
            prev_char,
            self.length,
            main_op_so_far,
            floor_ctx,
            prepared,
            &mut candidates,
        );

        for &ch in &candidates[..candidate_count] {
            let ch_idx = idx_of(ch);
            if !can_place_char(
                ch,
                ch_idx,
                index,
                prev_char,
                main_op_so_far,
                char_counts,
                floor_ctx,
                bracket_stack,
                stack_len,
                prepared,
                self.length,
                current_num_len,
                current_num_value,
                current_num_leading_zero,
            ) {
                continue;
            }

            expr[index] = ch;
            char_counts[ch_idx] += 1;

            let next_floor_ctx = update_floor_context(ch, floor_ctx);
            let mut new_main_lhs_value = main_lhs_value;
            let new_main_op = if is_main_operator_b(ch) {
                let Some(lhs_value) = evaluate_expression_solver_bytes(&expr[..index]) else {
                    char_counts[ch_idx] -= 1;
                    expr[index] = NO_CHAR;
                    continue;
                };
                if !is_integer(lhs_value) {
                    char_counts[ch_idx] -= 1;
                    expr[index] = NO_CHAR;
                    continue;
                }
                let lhs_value = lhs_value as i64;
                if ch == b'=' {
                    let rhs_slots = self.length - index - 1;
                    let mut rhs_buf = [0u8; 20];
                    let rhs_len = if lhs_value == 0 && rhs_slots == 2 {
                        rhs_buf[0] = b'-';
                        rhs_buf[1] = b'0';
                        2
                    } else {
                        write_i64_decimal(lhs_value, &mut rhs_buf)
                    };
                    if rhs_len == rhs_slots {
                        complete_eq_rhs(
                            self.length,
                            index,
                            expr,
                            char_counts,
                            prepared,
                            &rhs_buf[..rhs_len],
                            sink,
                            searched_count,
                        );
                    }
                    char_counts[ch_idx] -= 1;
                    expr[index] = NO_CHAR;
                    continue;
                }
                new_main_lhs_value = Some(lhs_value);
                Some(ch)
            } else {
                main_op_so_far
            };
            let (next_num_len, next_num_value, next_num_leading_zero) = if is_digit_b(ch) {
                if prev_char.is_some_and(is_digit_b) {
                    (
                        current_num_len + 1,
                        current_num_value * 10 + (ch - b'0') as i64,
                        current_num_leading_zero,
                    )
                } else {
                    (1, (ch - b'0') as i64, ch == b'0')
                }
            } else {
                (0, 0, false)
            };

            // Bracket stack is a shared scratch buffer reused across sibling
            // branches. A push writes bracket_stack[stack_len]; a deeper branch
            // that later closes this bracket and opens a different one can
            // overwrite this slot, so we must save the previous occupant and
            // restore it on backtrack — otherwise a sibling explored afterwards
            // (e.g. the `0`/`(`/`[` candidates, which come last) would read a
            // stale bracket type and wrongly reject a matching close bracket.
            let pushed_bracket = matches!(ch, b'(' | b'[');
            let saved_bracket_slot = if pushed_bracket {
                bracket_stack[stack_len]
            } else {
                NO_CHAR
            };
            let next_stack_len = match ch {
                b'(' | b'[' => {
                    bracket_stack[stack_len] = ch;
                    stack_len + 1
                }
                b')' | b']' => stack_len - 1,
                _ => stack_len,
            };

            self.recursive_search(
                index + 1,
                expr,
                Some(ch),
                new_main_op,
                if is_main_operator_b(ch) {
                    index
                } else {
                    main_op_index
                },
                new_main_lhs_value,
                char_counts,
                next_floor_ctx,
                bracket_stack,
                next_stack_len,
                prepared,
                next_num_len,
                next_num_value,
                next_num_leading_zero,
                sink,
                searched_count,
                branch_depth,
                branches,
            );

            if pushed_bracket {
                bracket_stack[stack_len] = saved_bracket_slot;
            }

            char_counts[ch_idx] -= 1;
            expr[index] = NO_CHAR;
        }
    }

    /// Get the top-level character branches for parallel execution
    pub fn get_top_level_branches(&self) -> Vec<(char, Option<char>, FloorContext)> {
        let char_counts = [0u8; CHARSET_LEN];
        let bracket_stack: Vec<u8> = Vec::new();

        let mut candidates = [NO_CHAR; CHARSET_LEN];
        let count = fill_candidate_chars(
            0,
            None,
            self.length,
            None,
            FloorContext::new(),
            &self.prepared,
            &mut candidates,
        );

        candidates[..count]
            .iter()
            .copied()
            .filter(|&ch| {
                can_place_char(
                    ch,
                    idx_of(ch),
                    0,
                    None,
                    None,
                    &char_counts,
                    FloorContext::new(),
                    &bracket_stack,
                    0,
                    &self.prepared,
                    self.length,
                    0,
                    0,
                    false,
                )
            })
            .map(|ch| {
                let main_op = if is_main_operator_b(ch) {
                    Some(ch as char)
                } else {
                    None
                };
                let floor_ctx = update_floor_context(ch, FloorContext::new());
                (ch as char, main_op, floor_ctx)
            })
            .collect()
    }

    /// Solve a single branch starting from a given first character
    pub fn solve_branch(
        &self,
        first_char: char,
        main_op: Option<char>,
        floor_ctx: FloorContext,
    ) -> (Vec<String>, u64) {
        // `solve_branch` is public and `first_char` may be arbitrary. Reject any
        // character outside the Sumzle charset before it reaches `idx_of`, which
        // would otherwise yield `INVALID_INDEX` (255) and index `char_counts`
        // out of bounds.
        if idx_of_char(first_char).is_none() {
            return (Vec::new(), 0);
        }
        let first = first_char as u8;
        if self.prepared.is_globally_forbidden(first)
            || self.prepared.cannot_be_at(0, first)
            || (self.prepared.fixed_chars[0] != NO_CHAR && self.prepared.fixed_chars[0] != first)
        {
            return (Vec::new(), 0);
        }

        let mut results: Vec<String> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
        let mut no_branches: Vec<Branch> = Vec::new();

        expr[0] = first;
        char_counts[idx_of(first)] += 1;
        let stack_len = match first {
            b'(' | b'[' => {
                bracket_stack[0] = first;
                1
            }
            _ => 0,
        };

        self.recursive_search(
            1,
            &mut expr,
            Some(first),
            main_op.map(|c| c as u8),
            0,
            None,
            &mut char_counts,
            floor_ctx,
            &mut bracket_stack,
            stack_len,
            &self.prepared,
            if is_digit_b(first) { 1 } else { 0 },
            if is_digit_b(first) {
                (first - b'0') as i64
            } else {
                0
            },
            first == b'0',
            &mut results,
            &mut searched_count,
            usize::MAX,
            &mut no_branches,
        );

        (results, searched_count)
    }

    /// Traverse the search tree, recording every node reached at `depth` as a
    /// `Branch` for parallel execution. Solutions whose main operator lands
    /// before `depth` (the directly-enumerated `=` equations) are found here
    /// and returned alongside, since those paths terminate before `depth`.
    ///
    /// Returns `(branches, eager_results, eager_searched)`.
    pub fn collect_branches_at_depth(&self, depth: usize) -> (Vec<Branch>, Vec<String>, u64) {
        let mut results: Vec<String> = Vec::new();
        let (branches, searched_count) = self.collect_branches_into(depth, &mut results);
        (branches, results, searched_count)
    }

    /// Like `collect_branches_at_depth`, but delivers the eagerly-found `=`
    /// solutions (those whose main operator lands before `depth`) to `sink`
    /// instead of a `Vec`. Returns `(branches, eager_searched)`.
    pub fn collect_branches_into<S: SolutionSink>(
        &self,
        depth: usize,
        sink: &mut S,
    ) -> (Vec<Branch>, u64) {
        let mut branches: Vec<Branch> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];

        self.recursive_search(
            0,
            &mut expr,
            None,
            None,
            0,
            None,
            &mut char_counts,
            FloorContext::new(),
            &mut bracket_stack,
            0,
            &self.prepared,
            0,
            0,
            false,
            sink,
            &mut searched_count,
            depth,
            &mut branches,
        );

        (branches, searched_count)
    }

    /// Resume the search from a `Branch` captured by `collect_branches_at_depth`,
    /// reconstructing the exact state `recursive_search` had at that node.
    pub fn solve_from_prefix(&self, branch: &Branch) -> (Vec<String>, u64) {
        let mut results: Vec<String> = Vec::new();
        let searched_count = self.solve_from_prefix_into(branch, &mut results);
        (results, searched_count)
    }

    /// Like `solve_from_prefix`, but delivers solutions to `sink`. Returns the
    /// number of complete expressions evaluated within this branch.
    pub fn solve_from_prefix_into<S: SolutionSink>(&self, branch: &Branch, sink: &mut S) -> u64 {
        let depth = branch.prefix.len();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        expr[..depth].copy_from_slice(&branch.prefix);
        let mut char_counts = [0u8; CHARSET_LEN];
        for &ch in &branch.prefix {
            char_counts[idx_of(ch)] += 1;
        }
        let stack_len = branch.bracket_stack.len();
        let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
        bracket_stack[..stack_len].copy_from_slice(&branch.bracket_stack);
        let prev_char = branch.prefix.last().copied();
        let mut no_branches: Vec<Branch> = Vec::new();

        self.recursive_search(
            depth,
            &mut expr,
            prev_char,
            branch.main_op,
            branch.main_op_index,
            branch.main_lhs_value,
            &mut char_counts,
            branch.floor_ctx,
            &mut bracket_stack,
            stack_len,
            &self.prepared,
            branch.num_len,
            branch.num_value,
            branch.num_leading_zero,
            sink,
            &mut searched_count,
            usize::MAX,
            &mut no_branches,
        );

        searched_count
    }
}
