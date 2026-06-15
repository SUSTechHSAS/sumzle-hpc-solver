//! Multi-core parallel solver using Rayon

use crate::solver::Solver;
use rayon::prelude::*;

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

    /// Solve using multiple threads via Rayon.
    ///
    /// The search is partitioned into many fine-grained prefix branches (far
    /// more than the ~11 top-level characters) so Rayon's work-stealing keeps
    /// every core busy despite the highly uneven branch sizes. This only
    /// changes how the identical search is divided — solutions and the total
    /// searched count match the single-threaded solver exactly.
    pub fn solve(&self) -> (Vec<String>, u64) {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            .build()
            .unwrap();

        // Deepen the prefix until there are comfortably more branches than
        // threads (or we run out of expression length).
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

        // Solve each branch independently; collect per-branch results without a
        // shared lock.
        let outcomes: Vec<(Vec<String>, u64)> = pool.install(|| {
            branches
                .par_iter()
                .map(|branch| self.solver.solve_from_prefix(branch))
                .collect()
        });

        let total_len = eager_results.len() + outcomes.iter().map(|(r, _)| r.len()).sum::<usize>();
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
}
