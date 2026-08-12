// Diagnostic: how many *distinct integer values* the right-hand side of `>`
// can take, per RHS length. `MAX_OPERAND_VALUE` bounds operands, but nested
// arithmetic can still push a result far higher, so this measures it directly.
//
// If the value set is small relative to the number of RHS *expressions*, the
// `lhs > rhs` join can be answered from a value histogram instead of scanning
// every right-hand side — the difference between O(LHS x RHS) and O(LHS + RHS).
use std::collections::HashMap;
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
    // Enumerate all expressions of each length that evaluate to an integer,
    // i.e. exactly the RHS candidate set, by solving `<len+1>` puzzles whose
    // whole body is a right-hand side. Simplest faithful proxy: solve the
    // puzzle and read the substring after the main `>`.
    for len in std::env::args()
        .skip(1)
        .filter_map(|a| a.parse::<usize>().ok())
    {
        let solver = Solver::new(len, empty_gk(len));
        let mut sink: Vec<String> = Vec::new();
        solver.solve_into(&mut sink);

        let mut values: HashMap<i64, u64> = HashMap::new();
        let mut rhs_exprs: HashMap<String, i64> = HashMap::new();
        for s in &sink {
            if let Some(pos) = s.find('>') {
                let rhs = &s[pos + 1..];
                if rhs_exprs.contains_key(rhs) {
                    continue;
                }
                if let Some(v) = evaluate_expression(rhs) {
                    if v.fract() == 0.0f64 {
                        rhs_exprs.insert(rhs.to_string(), v as i64);
                        *values.entry(v as i64).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut vs: Vec<i64> = values.keys().copied().collect();
        vs.sort_unstable();
        println!(
            "L={len}: distinct RHS exprs={} distinct RHS values={} range=[{:?}..{:?}]",
            rhs_exprs.len(),
            values.len(),
            vs.first(),
            vs.last()
        );
    }
}
