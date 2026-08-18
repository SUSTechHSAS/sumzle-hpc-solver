// Diagnostic: can the `lhs > rhs` join be answered from an aggregate index
// instead of by enumerating every (LHS, RHS) pair?
//
// Character statistics only need, per left-hand side, *how many* qualifying
// right-hand sides there are and which characters they contain — never the
// right-hand sides themselves. If the RHS set collapses to few distinct
// `(value, character-set)` pairs, those aggregates can be precomputed once and
// looked up per LHS, turning O(LHS x RHS) into O(LHS + RHS).
//
// This probe measures the collapse factor and the resulting index size.
use std::collections::{HashMap, HashSet};
use sumzle_solver::evaluator::evaluate_expression;
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
        let solver = Solver::new(len, empty_gk(len));
        let mut sink: Vec<String> = Vec::new();
        solver.solve_into(&mut sink);

        // Right-hand sides of the `>` equations, deduplicated.
        let mut rhs_set: HashSet<String> = HashSet::new();
        for s in &sink {
            if let Some(pos) = s.find('>') {
                rhs_set.insert(s[pos + 1..].to_string());
            }
        }

        let mut pairs: HashMap<(i64, u32), u64> = HashMap::new();
        let mut values: HashSet<i64> = HashSet::new();
        for rhs in &rhs_set {
            if let Some(v) = evaluate_expression(rhs) {
                if v.fract() != 0.0f64 {
                    continue;
                }
                let mut mask = 0u32;
                for ch in rhs.chars() {
                    mask |= 1u32 << (ch as u32 % 31);
                }
                values.insert(v as i64);
                *pairs.entry((v as i64, mask)).or_insert(0) += 1;
            }
        }

        // The index stores, per distinct value, a cumulative 24-lane character
        // histogram (u64 each) plus the value itself.
        let idx_bytes = values.len() * (8 + 24 * 8);
        println!(
            "L={len}: solutions={} rhs_exprs={} distinct_values={} distinct(value,mask)={} \
             collapse={:.0}x index={:.2} MB",
            sink.len(),
            rhs_set.len(),
            values.len(),
            pairs.len(),
            sink.len() as f64 / pairs.len().max(1) as f64,
            idx_bytes as f64 / 1e6
        );
    }
}
