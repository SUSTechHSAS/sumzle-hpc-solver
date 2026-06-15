//! Multi-core parallel solver using Rayon

use crate::solver::{CountSink, JsonlSink, SolutionSink, Solver, TopNSink};
use rayon::prelude::*;
use std::io::Write;
use std::sync::Mutex;

/// Parallel solver that distributes work across multiple CPU cores
pub struct ParallelSolver {
    pub solver: Solver,
    pub num_threads: usize,
}

impl ParallelSolver {
    pub fn new(solver: Solver, num_threads: Option<usize>) -> Self {
        let num_threads = num_threads.unwrap_or_else(num_cpus::get);
        Self {
            solver,
            num_threads,
        }
    }

    fn pool(&self) -> rayon::ThreadPool {
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            .build()
            .unwrap()
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
        let pool = self.pool();
        let (branches, mut eager_results, mut searched) = self.collect_branches();

        // Solve each branch independently; collect per-branch results without a
        // shared lock.
        let outcomes: Vec<(Vec<String>, u64)> = pool.install(|| {
            branches
                .par_iter()
                .map(|branch| self.solver.solve_from_prefix(branch))
                .collect()
        });

        let total_len =
            eager_results.len() + outcomes.iter().map(|(r, _)| r.len()).sum::<usize>();
        let mut results: Vec<String> = Vec::with_capacity(total_len);
        results.append(&mut eager_results);
        for (branch_results, branch_searched) in outcomes {
            searched += branch_searched;
            results.extend(branch_results);
        }

        // Branches partition the search space by prefix, so they never produce
        // duplicates; sort for a deterministic order (parallelized — the result
        // set can be millions of strings). The dedup is a defensive no-op.
        pool.install(|| results.par_sort_unstable());
        results.dedup();

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
    pub fn solve_to_writer<W: Write + Send>(&self, writer: W) -> std::io::Result<(u64, u64)> {
        let pool = self.pool();
        let (branches, eager_results, eager_searched) = self.collect_branches();

        let writer = Mutex::new(writer);

        // Eager `=` solutions found during branch collection.
        let mut esink = JsonlSink::new(&writer);
        for sol in &eager_results {
            esink.accept(sol.as_bytes());
        }
        let eager_written = esink.finish();

        let (branch_written, branch_searched): (u64, u64) = pool.install(|| {
            branches
                .par_iter()
                .map(|branch| {
                    let mut sink = JsonlSink::new(&writer);
                    let searched = self.solver.solve_from_prefix_into(branch, &mut sink);
                    (sink.finish(), searched)
                })
                .reduce(|| (0u64, 0u64), |a, b| (a.0 + b.0, a.1 + b.1))
        });

        let mut guard = writer.lock().expect("solution writer poisoned");
        guard.flush()?;

        Ok((
            eager_written + branch_written,
            eager_searched + branch_searched,
        ))
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
        let pool = self.pool();
        let (branches, eager_results, eager_searched) = self.collect_branches();

        // ---- Pass 1: character-frequency statistics over all solutions. ----
        let mut base_counts = CountSink::new();
        for sol in &eager_results {
            base_counts.accept(sol.as_bytes());
        }

        let (counts, branch_searched) = pool.install(|| {
            branches
                .par_iter()
                .map(|branch| {
                    let mut sink = CountSink::new();
                    let searched = self.solver.solve_from_prefix_into(branch, &mut sink);
                    (sink, searched)
                })
                .reduce(
                    || (CountSink::new(), 0u64),
                    |mut a, b| {
                        a.0.merge(&b.0);
                        a.1 += b.1;
                        a
                    },
                )
        });
        base_counts.merge(&counts);
        let searched = eager_searched + branch_searched;

        let probs = base_counts.probabilities();
        let top5 = base_counts.top5_mask();

        // ---- Pass 2: score every solution, keep the top n. ----
        let mut base_top = TopNSink::new(n, probs, top5);
        for sol in &eager_results {
            base_top.accept(sol.as_bytes());
        }

        let merged = pool.install(|| {
            branches
                .par_iter()
                .map(|branch| {
                    let mut sink = TopNSink::new(n, probs, top5);
                    self.solver.solve_from_prefix_into(branch, &mut sink);
                    sink
                })
                .reduce(
                    || TopNSink::new(n, probs, top5),
                    |mut a, b| {
                        a.merge(b);
                        a
                    },
                )
        });
        base_top.merge(merged);

        (base_top.into_sorted(), searched)
    }
}
