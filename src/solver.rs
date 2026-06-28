//! Brute-force search solver with pruning for Sumzle

use crate::evaluator::{evaluate_expression_solver_bytes, is_integer};
use crate::types::*;

const CHARSET_LEN: usize = 24;
const NO_CHAR: u8 = 0;
const INVALID_INDEX: u8 = u8::MAX;
/// L3: Maximum supported expression length for stack-allocated buffers.
const MAX_STACK_LEN: usize = 64;

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
// L1: Split AFTER_CLOSE_OR_FACTORIAL into three context-specific sets.
const AFTER_CLOSE_PAREN: &[u8] = b"+-*/%^A!)][=>";
const AFTER_CLOSE_FLOOR: &[u8] = b"+-*/%^A)][=>";
const AFTER_FACTORIAL: &[u8] = b"+-*/%^)][=>";
const AFTER_CLOSE_OR_FACTORIAL: &[u8] = b"+-*/%^A!)][=>";
const DEFAULT_ORDER: &[u8] = b"1234567890+-*/=([)]%^!A>";
const END_CHARS: &[u8] = b"0123456789)]!";
const LENGTH_ONE_DIGITS: &[u8] = b"0123456789";

// L1: Precomputed bitmasks (one bit per charset index, u32).
const END_CHARS_MASK: u32 = bytes_to_mask(END_CHARS);
const FLOOR_NO_SLASH_MASK: u32 = bytes_to_mask(FLOOR_NO_SLASH);
const FLOOR_WITH_SLASH_MASK: u32 = bytes_to_mask(FLOOR_WITH_SLASH);
const FLOOR_DIGITS_ONLY_MASK: u32 = bytes_to_mask(b"0123456789");
const AFTER_EQ_START_MASK: u32 = bytes_to_mask(AFTER_EQ_START);
const AFTER_EQ_MASK: u32 = bytes_to_mask(AFTER_EQ);
const FIRST_POSITION_MASK: u32 = bytes_to_mask(FIRST_POSITION);
const AFTER_DIGIT_MASK: u32 = bytes_to_mask(AFTER_DIGIT);
const AFTER_BINARY_OR_OPEN_MASK: u32 = bytes_to_mask(AFTER_BINARY_OR_OPEN);
const AFTER_CLOSE_PAREN_MASK: u32 = bytes_to_mask(AFTER_CLOSE_PAREN);
const AFTER_CLOSE_FLOOR_MASK: u32 = bytes_to_mask(AFTER_CLOSE_FLOOR);
const AFTER_FACTORIAL_MASK: u32 = bytes_to_mask(AFTER_FACTORIAL);
const DEFAULT_ORDER_MASK: u32 = bytes_to_mask(DEFAULT_ORDER);
const LENGTH_ONE_DIGITS_MASK: u32 = bytes_to_mask(LENGTH_ONE_DIGITS);
const OPEN_FLOOR_IDX: usize = idx_of_const(b'[');

const fn bytes_to_mask(bytes: &[u8]) -> u32 {
    let mut mask = 0u32;
    let mut i = 0usize;
    while i < bytes.len() {
        let idx = CHAR_INDEX[bytes[i] as usize];
        if idx != INVALID_INDEX {
            mask |= 1u32 << (idx as usize);
        }
        i += 1;
    }
    mask
}

const fn idx_of_const(ch: u8) -> usize {
    CHAR_INDEX[ch as usize] as usize
}

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
        Self {
            n,
            probs,
            top5_mask,
            heap: std::collections::BinaryHeap::new(),
        }
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
        // L4: Fast path for unconstrained puzzles.
        if self.constrained_indices.is_empty() {
            return true;
        }
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

// ===== L1+L2: Bitmask candidate generation + specialized dynamic checks =====

#[inline]
fn base_candidates_mask(
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
) -> u32 {
    if floor_ctx.in_floor {
        if !prev_char.is_some_and(is_digit_b) {
            FLOOR_DIGITS_ONLY_MASK
        } else if floor_ctx.has_slash_in_current_floor {
            FLOOR_WITH_SLASH_MASK
        } else {
            FLOOR_NO_SLASH_MASK
        }
    } else if main_op_so_far == Some(b'=') {
        if prev_char == Some(b'=') {
            AFTER_EQ_START_MASK
        } else {
            AFTER_EQ_MASK
        }
    } else if index == 0 {
        FIRST_POSITION_MASK
    } else if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            AFTER_DIGIT_MASK
        } else if is_binary_operator_b(pc) || is_open_bracket_b(pc) {
            AFTER_BINARY_OR_OPEN_MASK
        } else if pc == b')' {
            AFTER_CLOSE_PAREN_MASK
        } else if pc == b']' {
            AFTER_CLOSE_FLOOR_MASK
        } else if pc == b'!' {
            AFTER_FACTORIAL_MASK
        } else if is_main_operator_b(pc) {
            AFTER_BINARY_OR_OPEN_MASK
        } else {
            DEFAULT_ORDER_MASK
        }
    } else {
        DEFAULT_ORDER_MASK
    }
}

#[inline]
fn fill_candidate_mask(
    index: usize,
    prev_char: Option<u8>,
    length: usize,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
    prepared: &PreparedKnowledge,
) -> u32 {
    let blocked = prepared.globally_forbidden_mask | prepared.cannot_be_at_masks[index];
    let fixed = prepared.fixed_chars[index];
    if fixed != NO_CHAR {
        let fixed_mask = 1u32 << idx_of(fixed);
        if blocked & fixed_mask != 0 {
            return 0;
        }
        return fixed_mask;
    }
    let base = base_candidates_mask(index, prev_char, main_op_so_far, floor_ctx);
    let base = if index >= length.saturating_sub(3) {
        base & !(1u32 << OPEN_FLOOR_IDX)
    } else {
        base
    };
    if index == length - 1 && !floor_ctx.in_floor {
        let end_mask = base & END_CHARS_MASK;
        if end_mask != 0 {
            return end_mask & !blocked;
        }
        if prev_char.is_some() {
            return END_CHARS_MASK & !blocked;
        }
        if index == 0 && length == 1 {
            return LENGTH_ONE_DIGITS_MASK & !blocked;
        }
    }
    base & !blocked
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn dynamic_check(
    ch: u8,
    ch_idx: usize,
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    char_counts: &[u8; CHARSET_LEN],
    bracket_stack: &[u8],
    stack_len: usize,
    prepared: &PreparedKnowledge,
    length: usize,
    current_num_len: u8,
    current_num_value: i64,
    current_num_leading_zero: bool,
    floor_ctx: FloorContext,
) -> Option<usize> {
    // Exact count constraint (dynamic: depends on char_counts).
    if prepared.exact_mask & (1u32 << ch_idx) != 0
        && char_counts[ch_idx] >= prepared.exact_counts[ch_idx]
    {
        return None;
    }

    // Floor context constraints
    if floor_ctx.in_floor {
        if ch == b'[' || ch == b'(' || ch == b'A' || ch == b'!' {
            return None;
        }
        if is_operator_b(ch) && ch != b'/' {
            return None;
        }
        if is_main_operator_b(ch) {
            return None;
        }
        if ch == b'/' {
            if floor_ctx.has_slash_in_current_floor {
                return None;
            }
            if !prev_char.is_some_and(is_digit_b) || index == 0 {
                return None;
            }
        } else if ch == b']' {
            if !prev_char.is_some_and(is_digit_b) {
                return None;
            }
            if !floor_ctx.has_slash_in_current_floor {
                return None;
            }
        } else if !is_digit_b(ch) {
            return None;
        }
    }

    // Floor bracket constraints
    if ch == b'[' && floor_ctx.in_floor {
        return None;
    }
    if ch == b']' && !floor_ctx.in_floor {
        return None;
    }
    if ch == b'[' && index >= length.saturating_sub(3) {
        return None;
    }

    // Leading zero check and operand value check
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
            return None;
        }
        if main_op_so_far != Some(b'=') && new_value > MAX_OPERAND_VALUE {
            return None;
        }
    }

    // First position rules
    if index == 0
        && (is_binary_operator_b(ch)
            || is_close_bracket_b(ch)
            || is_main_operator_b(ch)
            || is_unary_post_operator_b(ch))
    {
        return None;
    }

    // Previous character-based rules
    if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            if is_open_bracket_b(ch) && ch != b'[' {
                return None;
            }
            if ch == b'[' && floor_ctx.in_floor {
                return None;
            }
        } else if is_operator_b(pc) {
            if is_binary_operator_b(ch)
                && !(pc == b'A' && (is_open_bracket_b(ch) || is_digit_b(ch)))
                && !is_unary_post_operator_b(pc)
            {
                return None;
            }
            if is_close_bracket_b(ch) && !is_unary_post_operator_b(pc) {
                return None;
            }
            if is_main_operator_b(ch) && !is_unary_post_operator_b(pc) {
                return None;
            }
            if is_unary_post_operator_b(pc) && (is_digit_b(ch) || is_open_bracket_b(ch)) {
                return None;
            }
        } else if is_open_bracket_b(pc) {
            if pc == b'[' && ch == b'(' {
                return None;
            }
            if is_binary_operator_b(ch) {
                return None;
            }
            if is_close_bracket_b(ch) && !matches_bracket(pc, ch) {
                return None;
            }
            if is_main_operator_b(ch) {
                return None;
            }
            if is_unary_post_operator_b(ch) {
                return None;
            }
        } else if is_close_bracket_b(pc) {
            if is_digit_b(ch) || is_open_bracket_b(ch) {
                return None;
            }
        } else if is_main_operator_b(pc) {
            if pc == b'=' {
                if !is_digit_b(ch) && ch != b'-' {
                    return None;
                }
            } else if is_main_operator_b(ch) || is_close_bracket_b(ch) {
                return None;
            }
        }
    }

    // After main operator =, only digits and minus
    if main_op_so_far == Some(b'=') {
        if !is_digit_b(ch) && ch != b'-' {
            return None;
        }
        if ch == b'-' && prev_char == Some(b'=') && index >= length - 1 {
            return None;
        }
    }

    // Last position rules
    if index == length - 1
        && (is_binary_operator_b(ch) || is_open_bracket_b(ch) || is_main_operator_b(ch))
    {
        return None;
    }

    // Incremental bracket balance check
    let new_stack_len = match ch {
        b'(' | b'[' => stack_len + 1,
        b')' | b']' => {
            if stack_len == 0 {
                return None;
            }
            let last_open = bracket_stack[stack_len - 1];
            if !matches_bracket(last_open, ch) {
                return None;
            }
            stack_len - 1
        }
        _ => stack_len,
    };
    if index == length - 1 && new_stack_len != 0 {
        return None;
    }

    // Main operator rules
    if is_main_operator_b(ch) {
        if main_op_so_far.is_some() {
            return None;
        }
        if index == 0 || index >= length - 1 {
            return None;
        }
    }

    // Permutation A rules
    if ch == b'A' && !prev_char.is_some_and(|pc| is_digit_b(pc) || is_close_bracket_b(pc)) {
        return None;
    }
    if prev_char == Some(b'A') && !is_digit_b(ch) && !is_open_bracket_b(ch) {
        return None;
    }

    // Factorial ! rules
    if ch == b'!' {
        if let Some(pc) = prev_char {
            if !is_digit_b(pc) && pc != b')' {
                return None;
            }
        } else {
            return None;
        }
    }

    Some(new_stack_len)
}

/// A3: Parse a pure-number RHS (digits, optional leading `-`) as i64.
#[inline]
fn parse_pure_number_rhs(rhs: &[u8]) -> Option<i64> {
    if rhs.is_empty() {
        return None;
    }
    let (negative, digits) = if rhs[0] == b'-' {
        if rhs.len() < 2 {
            return None;
        }
        (true, &rhs[1..])
    } else {
        (false, rhs)
    };
    let mut value: i64 = 0;
    for &b in digits {
        let d = (b - b'0') as i64;
        value = value.checked_mul(10)?.checked_add(d)?;
    }
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}

/// A3: Check if a byte slice is a pure number (all digits, or `-` followed by
/// all digits). A `-` anywhere except position 0 is a binary operator.
#[inline]
fn is_pure_number_bytes(rhs: &[u8]) -> bool {
    if rhs.is_empty() {
        return false;
    }
    let start = if rhs[0] == b'-' { 1 } else { 0 };
    if start == 1 && rhs.len() == 1 {
        return false;
    }
    rhs[start..].iter().all(|&b| b.is_ascii_digit())
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
        if let Some(pc) = prev_char {
            if !is_digit_b(pc) && pc != b')' {
                return false;
            }
        } else {
            return false;
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
}

impl Solver {
    pub fn new(length: usize, gk: GlobalKnowledge) -> Self {
        let prepared = PreparedKnowledge::new(length, &gk);
        Self {
            length,
            gk,
            prepared,
        }
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
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut no_branches: Vec<Branch> = Vec::new();

        // L3: stack-allocated arrays for length <= MAX_STACK_LEN.
        if self.length <= MAX_STACK_LEN {
            let mut expr_arr = [NO_CHAR; MAX_STACK_LEN];
            let mut bracket_arr = [NO_CHAR; MAX_STACK_LEN];
            self.recursive_search(
                0,
                &mut expr_arr[..self.length],
                None,
                None,
                0,
                None,
                &mut char_counts,
                FloorContext::new(),
                &mut bracket_arr[..self.length],
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
        } else {
            let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
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
                usize::MAX,
                &mut no_branches,
            );
        }

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

            let main_op = main_op_so_far.expect("main operator missing");
            let lhs_value = main_lhs_value.expect("main operator value missing");
            let right_side = &expr[main_op_index + 1..];
            let valid = match main_op {
                b'=' => false,
                b'>' => {
                    // A3: Fast path for pure-number RHS (no operators).
                    if is_pure_number_bytes(right_side) {
                        if let Some(rv) = parse_pure_number_rhs(right_side) {
                            is_integer(rv as f64) && lhs_value > rv
                        } else {
                            false
                        }
                    } else {
                        evaluate_expression_solver_bytes(right_side)
                            .is_some_and(|rv| is_integer(rv) && lhs_value > rv as i64)
                    }
                }
                _ => false,
            };

            if valid {
                sink.accept(expr);
            }
            return;
        }

        let candidate_mask = fill_candidate_mask(
            index,
            prev_char,
            self.length,
            main_op_so_far,
            floor_ctx,
            prepared,
        );

        let mut remaining = candidate_mask;
        while remaining != 0 {
            let ch_idx = remaining.trailing_zeros() as usize;
            let ch = CHAR_FROM_INDEX[ch_idx];
            remaining &= remaining - 1;

            let Some(next_stack_len) = dynamic_check(
                ch,
                ch_idx,
                index,
                prev_char,
                main_op_so_far,
                char_counts,
                bracket_stack,
                stack_len,
                prepared,
                self.length,
                current_num_len,
                current_num_value,
                current_num_leading_zero,
                floor_ctx,
            ) else {
                continue;
            };

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

            let pushed_bracket = matches!(ch, b'(' | b'[');
            let saved_bracket_slot = if pushed_bracket {
                bracket_stack[stack_len]
            } else {
                NO_CHAR
            };
            if pushed_bracket {
                bracket_stack[stack_len] = ch;
            }

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
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut no_branches: Vec<Branch> = Vec::new();

        // L3: stack-allocated arrays for length <= MAX_STACK_LEN.
        if self.length <= MAX_STACK_LEN {
            let mut expr_arr = [NO_CHAR; MAX_STACK_LEN];
            let mut bracket_arr = [NO_CHAR; MAX_STACK_LEN];
            expr_arr[0] = first;
            char_counts[idx_of(first)] += 1;
            let stack_len = match first {
                b'(' | b'[' => {
                    bracket_arr[0] = first;
                    1
                }
                _ => 0,
            };
            self.recursive_search(
                1,
                &mut expr_arr[..self.length],
                Some(first),
                main_op.map(|c| c as u8),
                0,
                None,
                &mut char_counts,
                floor_ctx,
                &mut bracket_arr[..self.length],
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
        } else {
            let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
            let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
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
        }

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
        let mut char_counts = [0u8; CHARSET_LEN];

        // L3: stack-allocated arrays for length <= MAX_STACK_LEN.
        if self.length <= MAX_STACK_LEN {
            let mut expr_arr = [NO_CHAR; MAX_STACK_LEN];
            let mut bracket_arr = [NO_CHAR; MAX_STACK_LEN];
            self.recursive_search(
                0,
                &mut expr_arr[..self.length],
                None,
                None,
                0,
                None,
                &mut char_counts,
                FloorContext::new(),
                &mut bracket_arr[..self.length],
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
        } else {
            let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
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
        }

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
        let mut char_counts = [0u8; CHARSET_LEN];
        for &ch in &branch.prefix {
            char_counts[idx_of(ch)] += 1;
        }
        let stack_len = branch.bracket_stack.len();
        let prev_char = branch.prefix.last().copied();
        let mut no_branches: Vec<Branch> = Vec::new();

        // L3: stack-allocated arrays for length <= MAX_STACK_LEN.
        if self.length <= MAX_STACK_LEN {
            let mut expr_arr = [NO_CHAR; MAX_STACK_LEN];
            expr_arr[..depth].copy_from_slice(&branch.prefix);
            let mut bracket_arr = [NO_CHAR; MAX_STACK_LEN];
            bracket_arr[..stack_len].copy_from_slice(&branch.bracket_stack);
            self.recursive_search(
                depth,
                &mut expr_arr[..self.length],
                prev_char,
                branch.main_op,
                branch.main_op_index,
                branch.main_lhs_value,
                &mut char_counts,
                branch.floor_ctx,
                &mut bracket_arr[..self.length],
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
        } else {
            let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
            expr[..depth].copy_from_slice(&branch.prefix);
            let mut bracket_stack: Vec<u8> = vec![NO_CHAR; self.length];
            bracket_stack[..stack_len].copy_from_slice(&branch.bracket_stack);
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
        }

        searched_count
    }
}
