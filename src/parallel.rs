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

    /// Solve using multiple threads via Rayon
    pub fn solve(&self) -> (Vec<String>, u64) {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            .build()
            .unwrap();

        let branches = self.solver.get_top_level_branches();

        let (mut results, searched_count) = pool.install(|| {
            branches
                .par_iter()
                .map(|&(first_char, main_op, floor_ctx)| {
                    self.solver.solve_branch(first_char, main_op, floor_ctx)
                })
                .reduce(
                    || (Vec::new(), 0),
                    |mut left, mut right| {
                        left.1 += right.1;
                        if !right.0.is_empty() {
                            left.0.append(&mut right.0);
                        }
                        left
                    },
                )
        });

        results.sort_unstable();
        results.dedup();

        (results, searched_count)
    }
}
