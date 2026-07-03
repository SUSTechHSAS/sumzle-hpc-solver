//! Multi-core parallel solver using Rayon

use crate::available_threads;
use crate::solver::{unique_char_mask, Branch, CountSink, SolutionSink, Solver, TopNSink};
use rayon::prelude::*;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

struct ChannelJsonlSink<'a> {
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    cancelled: &'a AtomicBool,
    writer_failed: &'a AtomicBool,
    buf: Vec<u8>,
    flush_at: usize,
    count: u64,
}

impl<'a> ChannelJsonlSink<'a> {
    fn new(
        tx: std::sync::mpsc::SyncSender<Vec<u8>>,
        cancelled: &'a AtomicBool,
        writer_failed: &'a AtomicBool,
    ) -> Self {
        const CAPACITY: usize = 256 * 1024;
        Self {
            tx,
            cancelled,
            writer_failed,
            buf: Vec::with_capacity(CAPACITY),
            flush_at: CAPACITY - 1024,
            count: 0,
        }
    }

    #[inline]
    fn is_stopped(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || self.writer_failed.load(Ordering::Relaxed)
    }

    fn flush_buf(&mut self) {
        if self.buf.is_empty() || self.is_stopped() {
            self.buf.clear();
            return;
        }

        let mut chunk = Vec::with_capacity(self.buf.capacity());
        std::mem::swap(&mut chunk, &mut self.buf);
        if self.tx.send(chunk).is_err() {
            self.writer_failed.store(true, Ordering::Relaxed);
        }
    }

    fn finish(mut self) -> u64 {
        self.flush_buf();
        self.count
    }
}

impl SolutionSink for ChannelJsonlSink<'_> {
    #[inline]
    fn accept(&mut self, expr: &[u8], _unique_mask: u32) {
        if self.is_stopped() {
            return;
        }
        self.buf.extend_from_slice(b"{\"solution\":\"");
        self.buf.extend_from_slice(expr);
        self.buf.extend_from_slice(b"\"}\n");
        self.count += 1;
        if self.buf.len() >= self.flush_at {
            self.flush_buf();
        }
    }
}

/// Live progress of a parallel solve, shared between the Rayon worker threads
/// (which bump `done` as each prefix branch finishes) and an outside observer
/// such as the SSE progress endpoint.
///
/// The unit of progress is a *branch* — the fine-grained prefix partitions that
/// [`ParallelSolver::collect_branches`] produces, which is exactly "the
/// multi-threaded task completion" the progress bar is meant to show. Updating
/// it costs one relaxed atomic add per branch, so it does not measurably
/// affect solve throughput.
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
        let num_threads = num_threads.unwrap_or_else(available_threads);
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
    /// The frontier is expanded breadth-first until there are comfortably more
    /// branches than threads (or we run out of depth), without re-running the
    /// search from the root at each candidate depth. Solutions whose main
    /// operator lands before the chosen depth terminate during branch
    /// collection and are returned as `eager_results`.
    fn collect_branches_with_target(
        &self,
        target_multiplier: usize,
        min_branches: usize,
    ) -> (Vec<Branch>, Vec<String>, u64) {
        let target = self
            .num_threads
            .saturating_mul(target_multiplier)
            .max(min_branches);
        let max_depth = self.solver.length.saturating_sub(1).max(1);
        let mut branches = vec![Branch::root()];
        let mut eager_results = Vec::new();
        let mut searched = 0u64;

        while branches.len() < target {
            let mut next = Vec::new();
            let mut expanded_any = false;

            for branch in branches.drain(..) {
                if branch.depth() >= max_depth {
                    next.push(branch);
                    continue;
                }

                let (children, branch_searched) = self
                    .solver
                    .collect_children_into(&branch, &mut eager_results);
                searched += branch_searched;
                if children.is_empty() {
                    continue;
                }
                expanded_any = true;
                next.extend(children);
            }

            branches = next;
            if !expanded_any {
                break;
            }
        }

        (branches, eager_results, searched)
    }

    #[inline]
    fn collect_branches(&self) -> (Vec<Branch>, Vec<String>, u64) {
        self.collect_branches_with_target(32, 32)
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

        let (mut branch_results, branch_searched) = self.run_in_pool(|| {
            branches
                .par_iter()
                .fold(
                    || (Vec::<String>::new(), 0u64),
                    |mut acc, branch| {
                        if progress.is_cancelled() {
                            return acc;
                        }
                        acc.1 += self.solver.solve_from_prefix_into(branch, &mut acc.0);
                        progress.inc_done();
                        acc
                    },
                )
                .reduce(
                    || (Vec::<String>::new(), 0u64),
                    |mut a, mut b| {
                        a.1 += b.1;
                        if a.0.len() < b.0.len() {
                            std::mem::swap(&mut a.0, &mut b.0);
                        }
                        a.0.append(&mut b.0);
                        a
                    },
                )
        });
        searched += branch_searched;

        eager_results.reserve(branch_results.len());
        eager_results.append(&mut branch_results);

        let mut results = self.run_in_pool(|| {
            eager_results.par_sort_unstable();
            eager_results
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
    /// cores searching to completion.
    pub fn solve_to_writer<W: Write + Send>(
        &self,
        writer: W,
        cancelled: &AtomicBool,
    ) -> std::io::Result<(u64, u64)> {
        let (branches, eager_results, eager_searched) = self.collect_branches_with_target(32, 32);
        let writer_failed = AtomicBool::new(false);
        let writer_error = Mutex::new(None::<std::io::Error>);
        let channel_capacity = self.num_threads.max(1).saturating_mul(8).max(16);
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(channel_capacity);

        let (written, searched) = std::thread::scope(|scope| {
            let writer_error_ref = &writer_error;
            let writer_failed_ref = &writer_failed;
            let writer_handle = scope.spawn(move || {
                let mut writer = writer;
                while let Ok(chunk) = rx.recv() {
                    if let Err(err) = writer.write_all(&chunk) {
                        writer_failed_ref.store(true, Ordering::Relaxed);
                        *writer_error_ref
                            .lock()
                            .expect("writer error mutex poisoned") = Some(err);
                        break;
                    }
                }
                if !writer_failed_ref.load(Ordering::Relaxed) {
                    if let Err(err) = writer.flush() {
                        writer_failed_ref.store(true, Ordering::Relaxed);
                        *writer_error_ref
                            .lock()
                            .expect("writer error mutex poisoned") = Some(err);
                    }
                }
            });

            let mut eager_sink = ChannelJsonlSink::new(tx.clone(), cancelled, &writer_failed);
            for sol in &eager_results {
                eager_sink.accept(sol.as_bytes(), 0);
            }
            let eager_written = eager_sink.finish();

            let partials: Vec<(u64, u64)> = self.run_in_pool(|| {
                branches
                    .par_iter()
                    .fold(
                        || {
                            (
                                ChannelJsonlSink::new(tx.clone(), cancelled, &writer_failed),
                                0u64,
                            )
                        },
                        |mut acc, branch| {
                            if acc.0.is_stopped() {
                                return acc;
                            }
                            acc.1 += self.solver.solve_from_prefix_into(branch, &mut acc.0);
                            acc
                        },
                    )
                    .map(|(sink, searched)| (sink.finish(), searched))
                    .collect()
            });

            drop(tx);
            let _ = writer_handle.join();

            let (branch_written, branch_searched) = partials
                .into_iter()
                .fold((0u64, 0u64), |(wa, sa), (wb, sb)| (wa + wb, sa + sb));

            (
                eager_written + branch_written,
                eager_searched + branch_searched,
            )
        });

        match writer_error
            .into_inner()
            .expect("writer error mutex poisoned")
        {
            Some(err) => Err(err),
            None => Ok((written, searched)),
        }
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
        let (branches, eager_results, eager_searched) = self.collect_branches_with_target(16, 16);
        progress.add_total(branches.len() as u64 * 2);
        progress.set_phase(PHASE_SEARCHING);

        let (base_top, base_counts, searched) = self.run_in_pool(|| {
            let mut base_counts = CountSink::new();
            for sol in &eager_results {
                base_counts.accept(sol.as_bytes(), unique_char_mask(sol.as_bytes()));
            }

            let (counts, branch_searched) = branches
                .par_iter()
                .fold(
                    || (CountSink::new(), 0u64),
                    |mut acc, branch| {
                        if progress.is_cancelled() {
                            return acc;
                        }
                        acc.1 += self.solver.solve_from_prefix_into(branch, &mut acc.0);
                        progress.inc_done();
                        acc
                    },
                )
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

            progress.set_phase(PHASE_SCORING);
            let mut base_top = TopNSink::new(n, probs, top5);
            for sol in &eager_results {
                base_top.accept(sol.as_bytes(), unique_char_mask(sol.as_bytes()));
            }

            let merged = branches
                .par_iter()
                .fold(
                    || TopNSink::new(n, probs, top5),
                    |mut sink, branch| {
                        if progress.is_cancelled() {
                            return sink;
                        }
                        self.solver.solve_from_prefix_into(branch, &mut sink);
                        progress.inc_done();
                        sink
                    },
                )
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
