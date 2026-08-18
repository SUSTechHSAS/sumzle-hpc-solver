//! Precomputed right-hand-side value index for the `>` main operator.
//!
//! # Why this exists
//!
//! For the `=` main operator the solver already resolves the whole right-hand
//! side in O(1): once the left-hand side evaluates to `v`, the only RHS that
//! can possibly work is the decimal spelling of `v`, so the search writes it
//! directly instead of enumerating characters (see `complete_eq_rhs`).
//!
//! The `>` operator had no such shortcut. For **every** LHS prefix the search
//! re-enumerated the entire RHS subtree character by character and re-parsed
//! each completed RHS from scratch, even though — with no positional
//! constraints — that subtree is *identical* for every prefix of the same
//! length. At length 8 that is 300k prefixes each re-walking the same few
//! hundred thousand RHS strings; the redundancy grows exponentially with the
//! puzzle length and is what makes long puzzles intractable.
//!
//! # What it does
//!
//! Each RHS length `k` is enumerated **once** into a table of
//! `(value, expression)` pairs sorted by value. A prefix whose LHS evaluates
//! to `v` then resolves its whole RHS subtree with a single binary search:
//! every entry with `value < v` is a solution, and they are exactly the first
//! `m` entries of the sorted table. Per-prefix cost drops from "walk the whole
//! subtree" to "one binary search", and the surviving range can be consumed in
//! bulk — counted with block prefix sums, or scanned with branch-and-bound for
//! top-N — without touching every element.
//!
//! # Memory
//!
//! The table is the only length-dependent allocation, and it is explicitly
//! capped (`memory_budget`). Construction stops as soon as the cap would be
//! exceeded and the caller transparently falls back to the recursive search,
//! so peak RSS stays under the budget at *any* puzzle length — the property
//! the streaming and top-N modes are built around. Indices are built for the
//! small RHS lengths first, which is where the redundancy is greatest (short
//! RHS ⇒ many prefixes sharing it).
//!
//! # Auxiliary structures
//!
//! Two small side tables make bulk consumption of a range cheap:
//!
//! * **Block character counts** — cumulative per-character solution counts at
//!   every `BLOCK`-th entry, so the character statistics of a range are read
//!   in `O(CHARSET_LEN + BLOCK)` instead of `O(m)`.
//! * **OR-tree** — a radix-`BLOCK` tree whose nodes hold the bitwise OR of the
//!   character masks beneath them. Because a node's OR is a superset of every
//!   mask below it, the top-N scorer can bound a whole subtree's best possible
//!   score from one `u32` and skip it outright.
//!
//! Both are independent of any per-solve scoring weights, so they are built
//! once with the index and reused by every consumer.

use crate::solver::CHARSET_LEN;

/// Fan-out of the auxiliary block structures (block character counts and the
/// OR-tree). A power of two so the index arithmetic is shifts and masks.
pub const BLOCK_SHIFT: u32 = 6;
/// Number of entries covered by one block / one OR-tree node.
pub const BLOCK: usize = 1 << BLOCK_SHIFT;

/// Per-entry bytes of the auxiliary structures, rounded up: the block
/// character counts contribute `CHARSET_LEN * 4 / BLOCK` bytes per entry and
/// the OR-tree just over `4 / BLOCK` bytes per entry (a geometric series).
const AUX_BYTES_PER_ENTRY: usize = (CHARSET_LEN * 4 + 8) / BLOCK + 1;

/// Fixed per-entry bytes: the value (`i64`) plus the character mask (`u32`).
const FIXED_BYTES_PER_ENTRY: usize = 8 + 4;

/// Per-entry bytes that exist only *while* [`RhsIndex::build`] runs: the
/// permutation and the two position-tracking vectors used to apply it in
/// place, at `u32` each.
///
/// The budget has to bound the peak, not just the finished structure, so this
/// transient cost is charged against it too — otherwise a build that ends up
/// inside the budget could still spike well past it partway through.
const BUILD_SCRATCH_BYTES_PER_ENTRY: usize = 3 * 4;

/// Bytes one index entry of RHS length `k` costs at the build-time peak,
/// including the expression text, the auxiliary structures and the temporary
/// sorting scratch. Used to enforce the memory budget.
#[inline]
pub const fn bytes_per_entry(k: usize) -> usize {
    FIXED_BYTES_PER_ENTRY + k + AUX_BYTES_PER_ENTRY + BUILD_SCRATCH_BYTES_PER_ENTRY
}

/// A fully enumerated, value-sorted table of the valid right-hand sides of a
/// given length for the `>` main operator.
///
/// Entries are sorted by `value` ascending, which is what turns the
/// `lhs > rhs` test into a prefix of the table (see [`Self::upper_bound`]).
pub struct RhsIndex {
    /// Length in characters of every RHS in this index.
    k: usize,
    /// Every syntactically valid RHS of this length that the recursive search
    /// would have reached, including those with a non-integer or unusable
    /// value that are not stored as entries. This is what the search would
    /// have added to `searched_count`, so resolving a prefix through the index
    /// keeps that statistic identical to the recursive search.
    total_leaves: u64,
    /// Entry values, ascending. Saturating `i64` of the evaluated `f64`, which
    /// is exactly the comparison the recursive search performs.
    values: Vec<i64>,
    /// Entry expressions, `k` bytes each, parallel to `values`.
    exprs: Vec<u8>,
    /// Distinct-character bitmask of each entry, parallel to `values`.
    masks: Vec<u32>,
    /// Cumulative per-character counts at block boundaries: row `b` (of
    /// `CHARSET_LEN` values) holds the counts over entries `[0, b * BLOCK)`.
    block_counts: Vec<u32>,
    /// OR-tree over `masks`. `or_levels[0][n]` is the OR of the masks in
    /// entries `[n * BLOCK, (n+1) * BLOCK)`, `or_levels[i+1][n]` the OR of
    /// `BLOCK` nodes of level `i`. The last level always has exactly one node.
    or_levels: Vec<Vec<u32>>,
}

impl RhsIndex {
    /// RHS length covered by this index.
    #[inline]
    pub fn k(&self) -> usize {
        self.k
    }

    /// Number of stored entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Complete expressions the recursive search would have evaluated for one
    /// prefix — the amount to add to `searched_count` when a prefix is
    /// resolved through this index instead of by recursion.
    #[inline]
    pub fn total_leaves(&self) -> u64 {
        self.total_leaves
    }

    /// The expression text of entry `i` (`k` bytes).
    #[inline]
    pub fn expr(&self, i: usize) -> &[u8] {
        let start = i * self.k;
        &self.exprs[start..start + self.k]
    }

    /// The distinct-character mask of entry `i`.
    #[inline]
    pub fn mask(&self, i: usize) -> u32 {
        self.masks[i]
    }

    /// Number of leading entries whose value is `< lhs_value` — i.e. the
    /// count of RHS that satisfy `lhs_value > rhs_value`. Since the table is
    /// sorted by value, the solutions for this prefix are exactly the entries
    /// `[0, upper_bound(lhs_value))`.
    #[inline]
    pub fn upper_bound(&self, lhs_value: i64) -> usize {
        self.values.partition_point(|&v| v < lhs_value)
    }

    /// Add the per-character solution counts of the entry range `[0, m)` into
    /// `counts`, counting each character once per entry.
    ///
    /// Reads the cumulative block table and scans only the partial trailing
    /// block, so the cost is `O(CHARSET_LEN + BLOCK)` regardless of `m`.
    pub fn add_char_counts_prefix(&self, m: usize, counts: &mut [u64; CHARSET_LEN]) {
        debug_assert!(m <= self.len());
        let full_blocks = m >> BLOCK_SHIFT;
        let row = full_blocks * CHARSET_LEN;
        for (c, count) in counts.iter_mut().enumerate() {
            *count += self.block_counts[row + c] as u64;
        }
        for i in (full_blocks << BLOCK_SHIFT)..m {
            let mut mask = self.masks[i];
            while mask != 0 {
                let bit = mask.trailing_zeros() as usize;
                counts[bit] += 1;
                mask &= mask - 1;
            }
        }
    }

    /// Number of levels in the OR-tree (0 when the index is empty).
    #[inline]
    pub fn or_levels(&self) -> usize {
        self.or_levels.len()
    }

    /// The OR of every entry mask beneath node `node` of level `level`, where
    /// level 0 is the first aggregated level (each node covering `BLOCK`
    /// entries). Entries themselves are reached through [`Self::mask`].
    #[inline]
    pub fn or_at(&self, level: usize, node: usize) -> u32 {
        self.or_levels[level][node]
    }

    /// Number of entries covered by one node of `level`.
    #[inline]
    pub fn span_of_level(level: usize) -> usize {
        1usize << (BLOCK_SHIFT as usize * (level + 1))
    }

    /// Total heap memory held by this index.
    pub fn heap_bytes(&self) -> usize {
        self.values.len() * 8
            + self.exprs.len()
            + self.masks.len() * 4
            + self.block_counts.len() * 4
            + self.or_levels.iter().map(|l| l.len() * 4).sum::<usize>()
    }

    /// Assemble an index from the raw enumeration output.
    ///
    /// `values` and `exprs` are in enumeration order; this sorts them by value
    /// (ties broken by enumeration order, so the result is deterministic) and
    /// derives the mask, block-count and OR-tree side tables.
    pub fn build(k: usize, total_leaves: u64, values: Vec<i64>, exprs: Vec<u8>) -> Self {
        let n = values.len();
        debug_assert_eq!(exprs.len(), n * k);

        // Sort by value, permuting in place.
        //
        // The obvious approach — build a permutation, then gather into fresh
        // `sorted_values`/`sorted_exprs` vectors — holds the original and the
        // sorted copy at the same time, roughly tripling peak memory while
        // building. That peak, not the finished index, is what a memory budget
        // actually has to cover, so instead the permutation is applied in
        // place by following its cycles: each element is moved directly to its
        // final slot, and the only extra allocations are the `u32` permutation
        // and a `k`-byte scratch buffer.
        let mut perm: Vec<u32> = (0..n as u32).collect();
        perm.sort_unstable_by(|&a, &b| {
            values[a as usize]
                .cmp(&values[b as usize])
                .then_with(|| a.cmp(&b))
        });

        let mut values = values;
        let mut exprs = exprs;
        // `pos[i]` tracks where original element `i` currently sits, and
        // `at[j]` which original element currently sits in slot `j`. Keeping
        // both lets every swap be resolved in O(1), so the whole permutation
        // is applied in O(n) swaps.
        let mut pos: Vec<u32> = (0..n as u32).collect();
        let mut at: Vec<u32> = (0..n as u32).collect();
        let mut scratch = vec![0u8; k];
        for slot in 0..n {
            let want = perm[slot];
            let cur = pos[want as usize] as usize;
            if cur != slot {
                values.swap(slot, cur);
                if k > 0 {
                    scratch.copy_from_slice(&exprs[slot * k..(slot + 1) * k]);
                    exprs.copy_within(cur * k..(cur + 1) * k, slot * k);
                    exprs[cur * k..(cur + 1) * k].copy_from_slice(&scratch);
                }

                let displaced = at[slot];
                at[slot] = want;
                at[cur] = displaced;
                pos[want as usize] = slot as u32;
                pos[displaced as usize] = cur as u32;
            }
        }
        drop(perm);
        drop(pos);
        drop(at);
        drop(scratch);

        let sorted_values = values;
        let sorted_exprs = exprs;
        let mut masks = Vec::with_capacity(n);
        for i in 0..n {
            masks.push(crate::solver::unique_char_mask(
                &sorted_exprs[i * k..(i + 1) * k],
            ));
        }

        // Cumulative per-character counts every BLOCK entries. Row `b` covers
        // `[0, b * BLOCK)`, so there are `n / BLOCK + 1` rows.
        let rows = (n >> BLOCK_SHIFT) + 1;
        let mut block_counts = vec![0u32; rows * CHARSET_LEN];
        {
            let mut running = [0u32; CHARSET_LEN];
            // Only *complete* blocks get a row: row `b + 1` is the cumulative
            // count over entries `[0, (b + 1) * BLOCK)`, which requires block
            // `b` to be full. A trailing partial chunk has no row of its own —
            // `add_char_counts_prefix` scans those entries directly — so it is
            // skipped here via `chunks_exact`, which yields only full chunks.
            for (b, chunk) in masks.chunks_exact(BLOCK).enumerate() {
                for &m in chunk {
                    let mut mask = m;
                    while mask != 0 {
                        let bit = mask.trailing_zeros() as usize;
                        running[bit] += 1;
                        mask &= mask - 1;
                    }
                }
                let row = (b + 1) * CHARSET_LEN;
                block_counts[row..row + CHARSET_LEN].copy_from_slice(&running);
            }
        }

        // OR-tree: level 0 aggregates BLOCK entries per node, each further
        // level aggregates BLOCK nodes of the level below, up to a single root.
        let mut or_levels: Vec<Vec<u32>> = Vec::new();
        if n > 0 {
            let mut level: Vec<u32> = Vec::with_capacity(n.div_ceil(BLOCK));
            for chunk in masks.chunks(BLOCK) {
                level.push(chunk.iter().fold(0u32, |a, &b| a | b));
            }
            or_levels.push(level);
            while or_levels.last().map_or(0, |l| l.len()) > 1 {
                let prev = or_levels.last().expect("non-empty");
                let mut next: Vec<u32> = Vec::with_capacity(prev.len().div_ceil(BLOCK));
                for chunk in prev.chunks(BLOCK) {
                    next.push(chunk.iter().fold(0u32, |a, &b| a | b));
                }
                or_levels.push(next);
            }
        }

        Self {
            k,
            total_leaves,
            values: sorted_values,
            exprs: sorted_exprs,
            masks,
            block_counts,
            or_levels,
        }
    }
}

/// The set of RHS indices for one puzzle, keyed by main-operator position.
///
/// `by_main_op_pos[p]` covers a `>` placed at index `p` (so an RHS of length
/// `length - p - 1`). `None` means "no index" — either the RHS length was out
/// of range or building it would have exceeded the memory budget — and the
/// caller falls back to the ordinary recursive search for that position.
pub struct RhsIndexSet {
    by_main_op_pos: Vec<Option<RhsIndex>>,
    total_bytes: usize,
}

impl RhsIndexSet {
    pub fn new(length: usize) -> Self {
        Self {
            by_main_op_pos: (0..length).map(|_| None).collect(),
            total_bytes: 0,
        }
    }

    /// The index to use for a `>` at position `pos`, if one was built.
    #[inline]
    pub fn get(&self, pos: usize) -> Option<&RhsIndex> {
        self.by_main_op_pos.get(pos).and_then(|e| e.as_ref())
    }

    pub fn insert(&mut self, pos: usize, index: RhsIndex) {
        self.total_bytes += index.heap_bytes();
        self.by_main_op_pos[pos] = Some(index);
    }

    /// Total memory held by all indices in this set.
    #[inline]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Whether any index was built at all.
    pub fn is_empty(&self) -> bool {
        self.by_main_op_pos.iter().all(|e| e.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_sorts_by_value_and_keeps_exprs_aligned() {
        // Three 1-char RHS with values out of order.
        let values = vec![7i64, 2, 5];
        let exprs = b"725".to_vec();
        let idx = RhsIndex::build(1, 3, values, exprs);

        assert_eq!(idx.len(), 3);
        assert_eq!(idx.k(), 1);
        assert_eq!(idx.total_leaves(), 3);
        assert_eq!(idx.expr(0), b"2");
        assert_eq!(idx.expr(1), b"5");
        assert_eq!(idx.expr(2), b"7");
    }

    #[test]
    fn upper_bound_counts_strictly_smaller_values() {
        let idx = RhsIndex::build(1, 3, vec![2i64, 5, 7], b"257".to_vec());
        assert_eq!(idx.upper_bound(0), 0);
        assert_eq!(idx.upper_bound(2), 0, "strict: 2 > 2 is false");
        assert_eq!(idx.upper_bound(3), 1);
        assert_eq!(idx.upper_bound(6), 2);
        assert_eq!(idx.upper_bound(8), 3);
        assert_eq!(idx.upper_bound(i64::MAX), 3);
    }

    #[test]
    fn char_counts_prefix_matches_manual_count() {
        // Enough entries to span several blocks and exercise the trailing scan.
        let n = BLOCK * 3 + 7;
        let values: Vec<i64> = (0..n as i64).collect();
        let exprs: Vec<u8> = (0..n).map(|i| b'0' + (i % 10) as u8).collect();
        let idx = RhsIndex::build(1, n as u64, values, exprs);

        for &m in &[0usize, 1, BLOCK - 1, BLOCK, BLOCK + 5, n - 1, n] {
            let mut got = [0u64; CHARSET_LEN];
            idx.add_char_counts_prefix(m, &mut got);

            let mut want = [0u64; CHARSET_LEN];
            for i in 0..m {
                let mut mask = idx.mask(i);
                while mask != 0 {
                    let bit = mask.trailing_zeros() as usize;
                    want[bit] += 1;
                    mask &= mask - 1;
                }
            }
            assert_eq!(got, want, "prefix char counts differ at m={m}");
        }
    }

    #[test]
    fn or_tree_nodes_superset_of_their_entries() {
        let n = BLOCK * BLOCK + 3;
        let values: Vec<i64> = (0..n as i64).collect();
        let exprs: Vec<u8> = (0..n).map(|i| b'0' + (i % 10) as u8).collect();
        let idx = RhsIndex::build(1, n as u64, values, exprs);

        assert!(idx.or_levels() >= 2, "expected a multi-level OR tree");
        for level in 0..idx.or_levels() {
            let span = RhsIndex::span_of_level(level);
            let nodes = idx.len().div_ceil(span);
            for node in 0..nodes {
                let or = idx.or_at(level, node);
                for i in (node * span)..((node + 1) * span).min(idx.len()) {
                    assert_eq!(
                        idx.mask(i) & !or,
                        0,
                        "entry mask must be a subset of its ancestor OR"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_index_is_well_formed() {
        let idx = RhsIndex::build(2, 0, Vec::new(), Vec::new());
        assert!(idx.is_empty());
        assert_eq!(idx.upper_bound(5), 0);
        assert_eq!(idx.or_levels(), 0);
        let mut counts = [0u64; CHARSET_LEN];
        idx.add_char_counts_prefix(0, &mut counts);
        assert_eq!(counts, [0u64; CHARSET_LEN]);
    }
}
