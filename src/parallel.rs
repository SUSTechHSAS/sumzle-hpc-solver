//! Multi-core parallel solver using Rayon

use crate::solver::Solver;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
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

    /// Solve using multiple threads via Rayon
    pub fn solve(&self) -> (Vec<String>, u64) {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            .build()
            .unwrap();

        let branches = self.solver.get_top_level_branches();
        let total_searched = AtomicU64::new(0);
        let results_mutex = Mutex::new(Vec::new());

        pool.install(|| {
            branches
                .par_iter()
                .for_each(|&(first_char, main_op, floor_ctx)| {
                    let (branch_results, branch_searched) =
                        self.solver.solve_branch(first_char, main_op, floor_ctx);

                    total_searched.fetch_add(branch_searched, Ordering::Relaxed);

                    if !branch_results.is_empty() {
                        let mut all_results = results_mutex.lock().unwrap();
                        all_results.extend(branch_results);
                    }
                });
        });

        let mut results = results_mutex.into_inner().unwrap();
        results.sort();
        results.dedup();

        (results, total_searched.into_inner())
    }
}
