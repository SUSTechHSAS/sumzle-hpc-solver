//! Multi-core parallel solver using Rayon

use crate::solver::{CountSink, JsonlSink, SolutionSink, Solver, TopNSink, CHARSET_LEN};
use rayon::prelude::*;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// Live progress of a parallel solve, shared between the Rayon worker threads
/// (which bump `done` as each prefix branch finishes) and an outside observer
/// such as the SSE progress endpoint.
///
/// The unit of progress is a *branch* — the fine-grained prefix partitions that
/// [`ParallelSolver::collect_branches`] produces, which is exactly "the
/// multi-threaded task completion" the progress bar is meant to show. Updating
/// it costs one relaxed atomic add per branch (there are ~16×threads branches),
/// so it does not measurably affect solve throughput.
#[derive(Debug)]
pub struct Progress {
    /// Branch-tasks completed so far.
    done: AtomicU64,
    /// Total branch-tasks to complete. 0 until the branch set is known; for the
    /// top-N two-pass solve this counts both passes (2 × branch count).
    total: AtomicU64,
    /// Coarse phase: 0 = preparing, 1 = searching (or pass 1), 2 = top-N
    /// scoring (pass 2), 3 = finished.
    phase: AtomicU8,
    /// Set when the consumer has gone away (e.g. the SSE client disconnected).
    /// Worker threads check this between branches and stop early so an abandoned
    /// solve does not keep all cores busy running to completion.
    cancelled: AtomicBool,
}

/// Phase constants for [`Progress::phase`].
pub const PHASE_PREPARING: u8 = 0;
pub const PHASE_SEARCHING: u8 = 1;
pub const PHASE_SCORING: u8 = 2;
pub const PHASE_DONE: u8 = 3;

impl Progress {
    pub fn new() -> Self {
        Self {
            done: AtomicU64::new(0),
            total: AtomicU64::new(0),
            phase: AtomicU8::new(PHASE_PREPARING),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Signal worker threads to stop early (the consumer has disconnected).
    #[inline]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](Self::cancel) has been called.
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    #[inline]
    fn add_total(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }

    #[inline]
    fn set_phase(&self, p: u8) {
        self.phase.store(p, Ordering::Relaxed);
    }

    #[inline]
    fn inc_done(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }

    /// `(done, total, phase)` read atomically-enough for display (relaxed loads;
    /// the values are monotonic so a torn read only ever under-reports briefly).
    pub fn snapshot(&self) -> (u64, u64, u8) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            self.phase.load(Ordering::Relaxed),
        )
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel solver that distributes work across multiple CPU cores
pub struct ParallelSolver {
    pub solver: Solver,
    pub num_threads: usize,
}

impl ParallelSolver {
    pub fn new(solver: Solver, num_threads: Option<usize>) -> Self {
        let num_threads = num_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });
        Self {
            solver,
            num_threads,
        }
    }

    /// Run `f` on a Rayon pool sized to `self.num_threads`.
    ///
    /// When the requested count already matches the ambient pool (the common
    /// case: `num_threads` defaulted to the CPU count, which is also Rayon's
    /// global-pool size), `f` runs directly on that pool — avoiding the cost of
    /// spawning and tearing down a fresh set of OS threads on every call. This
    /// matters in the server, where a solve happens per request. Only an
    /// explicit, differing thread count builds a transient pool.
    fn run_in_pool<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        if self.num_threads == rayon::current_num_threads() {
            f()
        } else {
            rayon::ThreadPoolBuilder::new()
                .num_threads(self.num_threads)
                .build()
                .expect("failed to build rayon thread pool")
                .install(f)
        }
    }

    /// Partition the search into fine-grained prefix branches.
    ///
    /// The prefix is deepened until there are comfortably more branches than
    /// threads (or we run out of expression length), so Rayon's work-stealing
    /// keeps every core busy despite highly uneven branch sizes. Solutions
    /// whose main operator lands before the chosen depth terminate during
    /// branch collection and are returned as `eager_results`.
    ///
    /// Each `collect_branches_at_depth` call re-runs the search from scratch,
    /// so the deeper call's `eager_results` is a superset of the shallower
    /// one's — keeping only the final depth's results is correct (not lossy).
    fn collect_branches(&self) -> (Vec<crate::solver::Branch>, Vec<String>, u64) {
        let target = self.num_threads.saturating_mul(16).max(16);
        let max_depth = self.solver.length.saturating_sub(1).max(1);
        let mut depth = 1;
        let (mut branches, mut eager_results, mut searched) =
            self.solver.collect_branches_at_depth(depth);
        while branches.len() < target && depth < max_depth {
            depth += 1;
            let next = self.solver.collect_branches_at_depth(depth);
            branches = next.0;
            eager_results = next.1;
            searched = next.2;
        }
        (branches, eager_results, searched)
    }

    /// Solve using multiple threads via Rayon.
    ///
    /// This only changes how the identical search is divided — solutions and
    /// the total searched count match the single-threaded solver exactly.
    pub fn solve(&self) -> (Vec<String>, u64) {
        self.solve_with_progress(&Progress::new())
    }

    /// Like [`solve`](Self::solve), but reports branch-completion progress
    /// through the shared `progress` handle (see [`Progress`]). The returned
    /// solutions and searched count are identical to `solve`.
    pub fn solve_with_progress(&self, progress: &Progress) -> (Vec<String>, u64) {
        let (branches, mut eager_results, mut searched) = self.collect_branches();
        progress.add_total(branches.len() as u64);
        progress.set_phase(PHASE_SEARCHING);

        // Solve every branch and assemble the sorted result set within a single
        // pool acquisition (both the per-branch search and the final sort run in
        // parallel).
        let mut results = self.run_in_pool(|| {
            let outcomes: Vec<(Vec<String>, u64)> = branches
                .par_iter()
                .map(|branch| {
                    // Skip remaining work if the consumer disconnected.
                    if progress.is_cancelled() {
                        return (Vec::new(), 0);
                    }
                    let out = self.solver.solve_from_prefix(branch);
                    progress.inc_done();
                    out
                })
                .collect();

            let total_len =
                eager_results.len() + outcomes.iter().map(|(r, _)| r.len()).sum::<usize>();
            let mut results: Vec<String> = Vec::with_capacity(total_len);
            results.append(&mut eager_results);
            for (branch_results, branch_searched) in outcomes {
                searched += branch_searched;
                results.extend(branch_results);
            }

            // Branches partition the search space by prefix, so they never
            // produce duplicates; sort for a deterministic order (parallelized —
            // the result set can be millions of strings).
            results.par_sort_unstable();
            results
        });

        // The dedup is a defensive no-op (branches are disjoint by prefix).
        results.dedup();
        progress.set_phase(PHASE_DONE);

        (results, searched)
    }

    /// Stream every solution to `writer` as JSON Lines (one `{"solution":"..."}`
    /// per line) without ever materializing the full solution set in memory.
    /// Returns the total number of complete expressions evaluated.
    ///
    /// Output order is unspecified (solutions are written as branches finish on
    /// arbitrary threads). Use the default `solve` or `solve_top_n` when a
    /// deterministic order is required.
    /// Returns `(solutions_written, searched_count)`.
    ///
    /// `cancelled` lets a consumer abort early: worker threads check it before
    /// each branch and stop, so a disconnected streaming client doesn't keep all
    /// cores searching to completion (Rayon's `map`/`reduce` does not itself
    /// short-circuit when a write returns `BrokenPipe`).
    pub fn solve_to_writer<W: Write + Send>(
        &self,
        writer: W,
        cancelled: &AtomicBool,
    ) -> std::io::Result<(u64, u64)> {
        let (branches, eager_results, eager_searched) = self.collect_branches();

        let writer = Mutex::new(writer);

        // Eager `=` solutions found during branch collection.
        let mut esink = JsonlSink::new(&writer);
        for sol in &eager_results {
            esink.accept(sol.as_bytes());
        }
        let eager_written = esink.finish()?;

        let (branch_written, branch_searched): (u64, u64) = self.run_in_pool(|| {
            branches
                .par_iter()
                .map(|branch| {
                    // Stop early if the client has gone away.
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok((0u64, 0u64));
                    }
                    let mut sink = JsonlSink::new(&writer);
                    let searched = self.solver.solve_from_prefix_into(branch, &mut sink);
                    sink.finish().map(|written| (written, searched))
                })
                .reduce(
                    || Ok((0u64, 0u64)),
                    |a, b| match (a, b) {
                        (Ok((wa, sa)), Ok((wb, sb))) => Ok((wa + wb, sa + sb)),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    },
                )
        })?;

        let mut guard = writer
            .lock()
            .map_err(|_| std::io::Error::other("solution writer mutex poisoned"))?;
        guard.flush()?;

        Ok((
            eager_written + branch_written,
            eager_searched + branch_searched,
        ))
    }

    /// The branch partition this solver would use. Diagnostics only.
    #[doc(hidden)]
    pub fn debug_branches(&self) -> Vec<crate::solver::Branch> {
        self.collect_branches().0
    }

    /// Count solutions with a purely thread-local sink — no shared writer, no
    /// allocation. Used to measure raw search throughput apart from the cost of
    /// delivering solutions. Returns `(found, searched)`.
    #[doc(hidden)]
    pub fn solve_tally(&self) -> (u64, u64) {
        #[derive(Default, Clone, Copy)]
        struct Tally {
            count: u64,
            checksum: u64,
        }
        impl SolutionSink for Tally {
            #[inline]
            fn accept(&mut self, expr: &[u8]) {
                self.count += 1;
                self.checksum = self.checksum.wrapping_add(expr[0] as u64);
            }

            #[inline]
            fn wants_aggregate(&self) -> bool {
                true
            }

            /// Counting needs no solution, only totals — and the checksum reads
            /// `expr[0]`, which lives in the shared prefix, so it folds too.
            #[inline]
            fn accept_aggregate(
                &mut self,
                prefix: &[u8],
                count: u64,
                _suffix_char_counts: &[u64; CHARSET_LEN],
            ) -> bool {
                self.count += count;
                self.checksum = self
                    .checksum
                    .wrapping_add((prefix[0] as u64).wrapping_mul(count));
                true
            }
        }

        let (branches, eager_results, eager_searched) = self.collect_branches();
        let (found, searched) = self.run_in_pool(|| {
            branches
                .par_iter()
                .map(|branch| {
                    let mut sink = Tally::default();
                    let searched = self.solver.solve_from_prefix_into(branch, &mut sink);
                    (sink.count, searched)
                })
                .reduce(|| (0u64, 0u64), |a, b| (a.0 + b.0, a.1 + b.1))
        });
        (
            found + eager_results.len() as u64,
            searched + eager_searched,
        )
    }

    /// Return the `n` most probable solutions, scored exactly as
    /// `server::compute_recommended` (sum of unique-character probabilities,
    /// plus 50 per global top-5 character). Memory is bounded by `O(threads * n)`
    /// — the full solution set is never stored.
    ///
    /// Two passes over the (deterministic) search: pass one accumulates the
    /// global character-frequency statistics the score depends on; pass two
    /// scores each solution and keeps the top `n`. Returns the kept solutions
    /// sorted by score descending (ties broken by expression ascending) and the
    /// total searched count.
    pub fn solve_top_n(&self, n: usize) -> (Vec<(f64, String)>, u64) {
        let (scored, _counts, searched) = self.solve_top_n_with_counts(n);
        (scored, searched)
    }

    /// Like [`solve_top_n`](Self::solve_top_n), but also returns the
    /// character-frequency statistics gathered in pass one over the *entire*
    /// solution set — not just the kept top-N. The web API uses these to report
    /// character probabilities across all solutions while returning only the
    /// ranked top-N subset (the top-N's own frequencies would be misleading).
    pub fn solve_top_n_with_counts(&self, n: usize) -> (Vec<(f64, String)>, CountSink, u64) {
        self.solve_top_n_with_counts_progress(n, &Progress::new())
    }

    /// Like [`solve_top_n_with_counts`](Self::solve_top_n_with_counts), but
    /// reports branch-completion progress through `progress`. Both passes are
    /// counted (so `total` is 2 × the branch count), and `phase` moves from
    /// [`PHASE_SEARCHING`] (pass 1) to [`PHASE_SCORING`] (pass 2).
    pub fn solve_top_n_with_counts_progress(
        &self,
        n: usize,
        progress: &Progress,
    ) -> (Vec<(f64, String)>, CountSink, u64) {
        let (branches, eager_results, eager_searched) = self.collect_branches();
        // Two passes over the same branch set.
        progress.add_total(branches.len() as u64 * 2);
        progress.set_phase(PHASE_SEARCHING);

        // Both passes share a single pool acquisition. The sequential glue
        // between them (folding eager solutions, deriving probabilities and the
        // top-5 mask) is cheap and just runs on the calling thread.
        let (base_top, base_counts, searched) = self.run_in_pool(|| {
            // ---- Pass 1: character-frequency statistics over all solutions. ----
            let mut base_counts = CountSink::new();
            for sol in &eager_results {
                base_counts.accept(sol.as_bytes());
            }

            let (counts, branch_searched) = branches
                .par_iter()
                .map(|branch| {
                    if progress.is_cancelled() {
                        return (CountSink::new(), 0);
                    }
                    let mut sink = CountSink::new();
                    let searched = self.solver.solve_from_prefix_into(branch, &mut sink);
                    progress.inc_done();
                    (sink, searched)
                })
                .reduce(
                    || (CountSink::new(), 0u64),
                    |mut a, b| {
                        a.0.merge(&b.0);
                        a.1 += b.1;
                        a
                    },
                );
            base_counts.merge(&counts);
            let searched = eager_searched + branch_searched;

            let probs = base_counts.probabilities();
            let top5 = base_counts.top5_mask();

            // ---- Pass 2: score every solution, keep the top n. ----
            progress.set_phase(PHASE_SCORING);

            // Pruning floor shared by every worker: whatever score the best-off
            // thread can already guarantee, the others may prune against. This
            // only skips subtrees that cannot enter the final top-n, so the
            // result is identical — it just avoids each thread having to climb
            // to a strong threshold on its own. Seeded from the eagerly
            // collected solutions so the branches start with a real cutoff.
            let floor = Arc::new(AtomicU64::new(f64::NEG_INFINITY.to_bits()));

            let mut base_top = TopNSink::new(n, probs, top5).with_shared_floor(Arc::clone(&floor));
            for sol in &eager_results {
                base_top.accept(sol.as_bytes());
            }
            base_top.publish_floor_now();

            let merged = branches
                .par_iter()
                .map(|branch| {
                    if progress.is_cancelled() {
                        return TopNSink::new(n, probs, top5);
                    }
                    let mut sink =
                        TopNSink::new(n, probs, top5).with_shared_floor(Arc::clone(&floor));
                    self.solver.solve_from_prefix_into(branch, &mut sink);
                    progress.inc_done();
                    sink
                })
                .reduce(
                    || TopNSink::new(n, probs, top5),
                    |mut a, b| {
                        a.merge(b);
                        a
                    },
                );
            base_top.merge(merged);

            (base_top, base_counts, searched)
        });

        progress.set_phase(PHASE_DONE);
        (base_top.into_sorted(), base_counts, searched)
    }
}
