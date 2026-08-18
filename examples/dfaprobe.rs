// Diagnostic: size and build cost of the RHS grammar DFA per length.
//
// The point of the DFA is that its node count tracks the *grammar state space*,
// which grows with the RHS length, and not the number of RHS expressions, which
// grows exponentially. This probe prints both so the gap is visible.
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
    // Force the byte cache off so every position falls back to the DFA — the
    // configuration that matters at extremely large lengths.
    std::env::set_var("SUMZLE_RHS_CACHE_MB", "0");

    for len in std::env::args()
        .skip(1)
        .filter_map(|a| a.parse::<usize>().ok())
    {
        let t0 = std::time::Instant::now();
        let solver = Solver::new(len, empty_gk(len));
        let build = t0.elapsed();
        let stats = solver.rhs_dfa_stats();
        let total_nodes: usize = stats.iter().map(|s| s.2).sum();
        println!(
            "L={len}: build {:?}, {} DFAs, {} nodes total (~{:.1} MB)",
            build,
            stats.len(),
            total_nodes,
            // Each node: two Vecs (24 B header each) plus a handful of entries.
            (total_nodes * (48 + 12 * 5)) as f64 / 1e6
        );
        for (k, rhs_len, nodes, leaves) in stats {
            println!("   k={k:2} rhs_len={rhs_len:2} nodes={nodes:8} leaves={leaves}");
        }
        let vi = solver.rhs_value_index_stats();
        let vi_bytes: usize = vi.iter().map(|s| s.2).sum();
        println!(
            "   value indexes: {} positions, {:.2} MB total",
            vi.len(),
            vi_bytes as f64 / 1e6
        );
        for (k, values, bytes) in vi {
            println!(
                "     k={k:2} distinct_values={values:7} {:.2} MB",
                bytes as f64 / 1e6
            );
        }
    }
}
