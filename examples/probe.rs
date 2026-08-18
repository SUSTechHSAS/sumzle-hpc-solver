// Diagnostic: memory footprint of the cached RHS tables per length.
use std::collections::HashMap;
use sumzle_solver::solver::Solver;
use sumzle_solver::types::*;

fn empty_gk(len: usize) -> GlobalKnowledge {
    GlobalKnowledge {
        fixed_chars: vec![None; len],
        cannot_be_at: vec![Default::default(); len],
        must_appear_min_count: HashMap::new(),
        must_appear_exact_count: HashMap::new(),
        globally_forbidden: Default::default(),
    }
}

fn main() {
    for len in std::env::args()
        .skip(1)
        .filter_map(|a| a.parse::<usize>().ok())
    {
        let t0 = std::time::Instant::now();
        let solver = Solver::new(len, empty_gk(len));
        let build = t0.elapsed();
        let stats = solver.rhs_table_stats();
        let total: usize = stats.iter().map(|s| s.3).sum();
        println!(
            "L={len}: build {:?}, total table mem {:.1} MB",
            build,
            total as f64 / 1e6
        );
        for (k, rhs_len, entries, bytes) in stats {
            println!(
                "   k={k} rhs_len={rhs_len}: {entries} entries, {:.1} MB",
                bytes as f64 / 1e6
            );
        }
    }
}
