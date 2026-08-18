//! Comprehensive test suite for behavioral consistency with the reference JavaScript implementation

use sumzle_solver::evaluator::{
    check_brackets, evaluate_expression, is_integer, is_simple_number_or_negative,
    is_valid_equation,
};
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::Solver;
use sumzle_solver::types::*;

/// Helper to build a GlobalKnowledge with no constraints
fn empty_gk(length: usize) -> GlobalKnowledge {
    GlobalKnowledge {
        fixed_chars: vec![None; length],
        cannot_be_at: vec![std::collections::HashSet::new(); length],
        must_appear_min_count: std::collections::HashMap::new(),
        must_appear_exact_count: std::collections::HashMap::new(),
        globally_forbidden: std::collections::HashSet::new(),
    }
}

/// Helper to build a GlobalKnowledge with a single guess row
fn gk_from_row(length: usize, tiles: &[(char, TileState)]) -> GlobalKnowledge {
    let row: GuessRow = tiles
        .iter()
        .map(|&(ch, state)| Tile { char: ch, state })
        .collect();
    GlobalKnowledge::from_guess_rows(length, &[row]).unwrap()
}

// =========================================================================
// Expression Evaluation Tests (matching JS behavior)
// =========================================================================

#[test]
fn test_basic_arithmetic() {
    assert_eq!(evaluate_expression("1+2"), Some(3.0));
    assert_eq!(evaluate_expression("5-3"), Some(2.0));
    assert_eq!(evaluate_expression("3*4"), Some(12.0));
    assert_eq!(evaluate_expression("10/2"), Some(5.0));
    assert_eq!(evaluate_expression("7%3"), Some(1.0));
}

#[test]
fn test_power() {
    assert_eq!(evaluate_expression("2^3"), Some(8.0));
    assert_eq!(evaluate_expression("2^10"), Some(1024.0));
    assert_eq!(evaluate_expression("3^2"), Some(9.0));
}

#[test]
fn test_factorial() {
    assert_eq!(evaluate_expression("0!"), Some(1.0));
    assert_eq!(evaluate_expression("1!"), Some(1.0));
    assert_eq!(evaluate_expression("5!"), Some(120.0));
    assert_eq!(evaluate_expression("10!"), Some(3628800.0));
    assert_eq!(evaluate_expression("12!"), Some(479001600.0));
    // 13! is too large
    assert_eq!(evaluate_expression("13!"), None);
}

#[test]
fn test_permutation() {
    assert_eq!(evaluate_expression("5A3"), Some(60.0));
    assert_eq!(evaluate_expression("10A2"), Some(90.0));
    assert_eq!(evaluate_expression("3A3"), Some(6.0));
    assert_eq!(evaluate_expression("1A1"), Some(1.0));
    assert_eq!(evaluate_expression("2A5"), None);
}

#[test]
fn test_floor_brackets() {
    assert_eq!(evaluate_expression("[7/2]"), Some(3.0));
    assert_eq!(evaluate_expression("[5]"), Some(5.0));
    assert_eq!(evaluate_expression("[10/3]"), Some(3.0));
}

#[test]
fn test_brackets_check() {
    assert!(check_brackets("(1+2)"));
    assert!(check_brackets("[7/2]"));
    assert!(check_brackets("((1+2))"));
    assert!(check_brackets("1+2"));
    assert!(!check_brackets("(1+2"));
    assert!(!check_brackets("1+2)"));
    assert!(!check_brackets("(1+2]"));
    assert!(!check_brackets("[1+2)"));
}

#[test]
fn test_operator_precedence() {
    assert_eq!(evaluate_expression("1+2*3"), Some(7.0));
    assert_eq!(evaluate_expression("2*3+1"), Some(7.0));
}

#[test]
fn test_parentheses() {
    assert_eq!(evaluate_expression("(1+2)*3"), Some(9.0));
    assert_eq!(evaluate_expression("(2+3)*(4+1)"), Some(25.0));
}

#[test]
fn test_leading_zeros() {
    assert_eq!(evaluate_expression("01"), None);
    assert_eq!(evaluate_expression("007"), None);
    assert_eq!(evaluate_expression("0"), Some(0.0));
    assert_eq!(evaluate_expression("10"), Some(10.0));
    assert_eq!(evaluate_expression("100"), Some(100.0));
}

#[test]
fn test_division_by_zero() {
    assert_eq!(evaluate_expression("1/0"), None);
}

#[test]
fn test_integer_check() {
    assert!(is_integer(5.0));
    assert!(is_integer(-3.0));
    assert!(is_integer(0.0));
    assert!(!is_integer(3.5));
    assert!(!is_integer(f64::NAN));
    assert!(!is_integer(f64::INFINITY));
}

// =========================================================================
// Equation Validation Tests
// =========================================================================

#[test]
fn test_simple_equations() {
    assert!(is_valid_equation("1+2=3"));
    assert!(is_valid_equation("2*3=6"));
    assert!(is_valid_equation("10-3=7"));
}

#[test]
fn test_invalid_no_main_op() {
    assert!(!is_valid_equation("123"));
    assert!(!is_valid_equation("1+2"));
}

#[test]
fn test_rhs_must_be_simple_number() {
    assert!(is_valid_equation("2*3=6"));
    assert!(!is_valid_equation("6=2*3"));
}

#[test]
fn test_negative_rhs() {
    assert!(is_valid_equation("3-5=-2"));
    assert!(!is_valid_equation("3-5=-2+1"));
}

#[test]
fn test_greater_than() {
    assert!(is_valid_equation("5>3"));
    assert!(!is_valid_equation("3>5"));
    assert!(!is_valid_equation("5>5"));
}

#[test]
fn test_greater_equal() {
    assert!(is_valid_equation("5>=5"));
    assert!(is_valid_equation("5>=3"));
    assert!(!is_valid_equation("3>=5"));
}

#[test]
fn test_factorial_equations() {
    assert!(is_valid_equation("5!=120"));
    assert!(is_valid_equation("3!*2=12"));
}

#[test]
fn test_permutation_equations() {
    assert!(is_valid_equation("5A3=60"));
}

#[test]
fn test_floor_equations() {
    assert!(is_valid_equation("[7/2]=3"));
    assert!(is_valid_equation("[7/2]*2=6"));
}

#[test]
fn test_is_simple_number() {
    assert!(is_simple_number_or_negative("5"));
    assert!(is_simple_number_or_negative("-3"));
    assert!(is_simple_number_or_negative("100"));
    assert!(!is_simple_number_or_negative("2*3"));
    assert!(!is_simple_number_or_negative("(5)"));
}

// =========================================================================
// Constraint Processing Tests
// =========================================================================

#[test]
fn test_empty_constraints() {
    let gk = empty_gk(6);
    assert!(gk.fixed_chars.iter().all(|c| c.is_none()));
    assert!(gk.globally_forbidden.is_empty());
}

#[test]
fn test_correct_constraint() {
    let gk = gk_from_row(
        6,
        &[
            ('1', TileState::Correct),
            ('+', TileState::Empty),
            ('2', TileState::Present),
            ('=', TileState::Empty),
            ('3', TileState::Empty),
            ('0', TileState::Empty),
        ],
    );
    assert_eq!(gk.fixed_chars[0], Some('1'));
    assert!(gk.cannot_be_at[0].contains(&'+'));
}

#[test]
fn test_present_constraint() {
    let gk = gk_from_row(
        6,
        &[
            ('1', TileState::Empty),
            ('+', TileState::Present),
            ('2', TileState::Empty),
            ('=', TileState::Empty),
            ('3', TileState::Empty),
            ('0', TileState::Empty),
        ],
    );
    assert!(gk.cannot_be_at[1].contains(&'+'));
    assert!(gk.must_appear_min_count.contains_key(&'+'));
    assert!(*gk.must_appear_min_count.get(&'+').unwrap() >= 1);
}

#[test]
fn test_absent_constraint() {
    let gk = gk_from_row(
        6,
        &[
            ('1', TileState::Empty),
            ('+', TileState::Empty),
            ('2', TileState::Empty),
            ('=', TileState::Empty),
            ('3', TileState::Empty),
            ('4', TileState::Empty),
        ],
    );
    // All chars with state Empty should be at least at their positions excluded
    assert!(gk.cannot_be_at[0].contains(&'1'));
}

#[test]
fn test_conflicting_fixed_chars() {
    let row1: GuessRow = vec![
        Tile {
            char: '1',
            state: TileState::Correct,
        },
        Tile {
            char: '+',
            state: TileState::Empty,
        },
        Tile {
            char: '2',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Empty,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        },
    ];
    let row2: GuessRow = vec![
        Tile {
            char: '2',
            state: TileState::Correct,
        },
        Tile {
            char: '+',
            state: TileState::Empty,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Empty,
        },
        Tile {
            char: '5',
            state: TileState::Empty,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        },
    ];
    let result = GlobalKnowledge::from_guess_rows(6, &[row1, row2]);
    assert!(result.is_err());
}

// =========================================================================
// Solver Correctness Tests
// =========================================================================

#[test]
fn test_solve_length_6_no_constraints() {
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, searched_count) = solver.solve();

    assert!(!results.is_empty(), "Should find at least one solution");
    assert!(searched_count > 0);

    for sol in &results {
        assert!(
            is_valid_equation(sol),
            "Solution '{}' should be a valid equation",
            sol
        );
        assert_eq!(sol.len(), 6, "Solution '{}' should have length 6", sol);
    }
}

#[test]
fn test_solve_with_correct_constraint() {
    let row: GuessRow = vec![
        Tile {
            char: '1',
            state: TileState::Correct,
        },
        Tile {
            char: '+',
            state: TileState::Empty,
        },
        Tile {
            char: '2',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Correct,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        },
    ];
    let gk = GlobalKnowledge::from_guess_rows(6, &[row]).unwrap();
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    for sol in &results {
        assert!(
            sol.starts_with('1'),
            "Solution '{}' should start with '1'",
            sol
        );
        assert!(
            sol.as_bytes()[3] == b'=',
            "Solution '{}' should have '=' at position 3",
            sol
        );
    }
}

#[test]
fn test_solve_with_present_constraint() {
    let row: GuessRow = vec![
        Tile {
            char: '1',
            state: TileState::Empty,
        },
        Tile {
            char: '+',
            state: TileState::Present,
        },
        Tile {
            char: '2',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Empty,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        },
    ];
    let gk = GlobalKnowledge::from_guess_rows(6, &[row]).unwrap();
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    for sol in &results {
        assert!(sol.contains('+'), "Solution '{}' should contain '+'", sol);
        assert_ne!(
            sol.as_bytes()[1],
            b'+',
            "Solution '{}' should not have '+' at position 1",
            sol
        );
    }
}

#[test]
fn test_solve_specific_equation() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();

    assert!(
        results.contains(&"1+2=3".to_string()),
        "Should find '1+2=3'"
    );
    assert!(
        results.contains(&"2*3=6".to_string()),
        "Should find '2*3=6'"
    );
}

#[test]
fn test_no_duplicate_solutions() {
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    let mut sorted = results.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        results.len(),
        sorted.len(),
        "No duplicate solutions should exist"
    );
}

// =========================================================================
// Parallel Solver Consistency Tests
// =========================================================================

#[test]
fn test_parallel_matches_sequential() {
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);

    let (seq_results, _seq_searched) = solver.solve();

    let parallel_solver = ParallelSolver::new(solver, Some(2));
    let (par_results, _par_searched) = parallel_solver.solve();

    let mut seq_sorted = seq_results;
    seq_sorted.sort();
    let mut par_sorted = par_results;
    par_sorted.sort();

    assert_eq!(
        seq_sorted, par_sorted,
        "Parallel and sequential results should match"
    );
}

#[test]
fn test_parallel_matches_sequential_length7() {
    let gk = empty_gk(7);
    let solver = Solver::new(7, gk);

    let (seq_results, _seq_searched) = solver.solve();

    let parallel_solver = ParallelSolver::new(solver, Some(2));
    let (par_results, _par_searched) = parallel_solver.solve();

    let mut seq_sorted = seq_results;
    seq_sorted.sort();
    let mut par_sorted = par_results;
    par_sorted.sort();

    assert_eq!(
        seq_sorted, par_sorted,
        "Parallel and sequential results should match for length 7"
    );
}

#[test]
fn test_parallel_matches_sequential_length8() {
    let gk = empty_gk(8);
    let solver = Solver::new(8, gk);

    let (seq_results, _seq_searched) = solver.solve();

    let parallel_solver = ParallelSolver::new(solver, Some(4));
    let (par_results, _par_searched) = parallel_solver.solve();

    let mut seq_sorted = seq_results;
    seq_sorted.sort();
    let mut par_sorted = par_results;
    par_sorted.sort();

    assert_eq!(
        seq_sorted, par_sorted,
        "Parallel and sequential results should match for length 8"
    );
}

#[test]
fn test_parallel_with_constraints() {
    let row: GuessRow = vec![
        Tile {
            char: '2',
            state: TileState::Correct,
        },
        Tile {
            char: '*',
            state: TileState::Present,
        },
        Tile {
            char: '3',
            state: TileState::Correct,
        },
        Tile {
            char: '=',
            state: TileState::Correct,
        },
        Tile {
            char: '6',
            state: TileState::Correct,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        },
    ];
    let gk = GlobalKnowledge::from_guess_rows(6, &[row]).unwrap();

    let solver = Solver::new(6, gk);
    let (seq_results, _) = solver.solve();

    let parallel_solver = ParallelSolver::new(solver, Some(2));
    let (par_results, _) = parallel_solver.solve();

    let mut seq_sorted = seq_results;
    seq_sorted.sort();
    let mut par_sorted = par_results;
    par_sorted.sort();

    assert_eq!(seq_sorted, par_sorted);
}

// =========================================================================
// Character Classification Tests
// =========================================================================

#[test]
fn test_char_classification() {
    assert!(is_digit('0'));
    assert!(is_digit('9'));
    assert!(!is_digit('+'));
    assert!(!is_digit('a'));

    assert!(is_binary_operator('+'));
    assert!(is_binary_operator('-'));
    assert!(is_binary_operator('*'));
    assert!(is_binary_operator('/'));
    assert!(is_binary_operator('%'));
    assert!(is_binary_operator('^'));
    assert!(is_binary_operator('A'));
    assert!(!is_binary_operator('='));

    assert!(is_unary_post_operator('!'));
    assert!(!is_unary_post_operator('+'));

    assert!(is_main_operator('='));
    assert!(is_main_operator('>'));
    assert!(!is_main_operator('+'));

    assert!(is_open_bracket('('));
    assert!(is_open_bracket('['));
    assert!(!is_open_bracket(')'));

    assert!(is_close_bracket(')'));
    assert!(is_close_bracket(']'));
    assert!(!is_close_bracket('('));
}

// =========================================================================
// Edge Cases
// =========================================================================

#[test]
fn test_single_digit_equation() {
    let gk = empty_gk(3);
    let solver = Solver::new(3, gk);
    let (results, _searched) = solver.solve();

    assert!(!results.is_empty());
    for sol in &results {
        assert_eq!(sol.len(), 3);
        assert!(is_valid_equation(sol));
    }
}

#[test]
fn test_contradictory_constraints() {
    let row1: GuessRow = vec![
        Tile {
            char: '1',
            state: TileState::Correct,
        },
        Tile {
            char: '+',
            state: TileState::Empty,
        },
        Tile {
            char: '2',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Empty,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
    ];
    let row2: GuessRow = vec![
        Tile {
            char: '5',
            state: TileState::Correct,
        },
        Tile {
            char: '+',
            state: TileState::Empty,
        },
        Tile {
            char: '3',
            state: TileState::Empty,
        },
        Tile {
            char: '=',
            state: TileState::Empty,
        },
        Tile {
            char: '8',
            state: TileState::Empty,
        },
    ];
    let result = GlobalKnowledge::from_guess_rows(5, &[row1, row2]);
    assert!(result.is_err());
}

#[test]
fn test_solve_length_5() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();
    assert!(!results.is_empty());
    for sol in &results {
        assert!(is_valid_equation(sol));
        assert_eq!(sol.len(), 5);
    }
    assert!(results.contains(&"1+2=3".to_string()));
}

#[test]
fn test_solver_does_not_generate_greater_equal() {
    for length in 3..=8 {
        let solver = Solver::new(length, empty_gk(length));
        let (results, _) = solver.solve();
        assert!(
            results.iter().all(|s| !s.contains(">=")),
            "Solver should not generate >= main operators for length {}",
            length
        );
    }
}

// =========================================================================
// Factorial Enumeration Tests (Bug Fix: '!' operator with empty constraints)
// =========================================================================

/// Test that factorial equations with '=' are enumerated when constraints are empty.
/// This was the core bug: after '!', the solver only offered digits and open brackets
/// as candidates (treating '!' as a binary operator), preventing expressions like
/// "3!=6" from ever being generated.
#[test]
fn test_factorial_equations_with_equals_no_constraints() {
    // Length 4 should include 1!=1, 2!=2, 3!=6
    let gk = empty_gk(4);
    let solver = Solver::new(4, gk);
    let (results, _searched) = solver.solve();

    assert!(results.contains(&"1!=1".to_string()), "Should find '1!=1'");
    assert!(results.contains(&"2!=2".to_string()), "Should find '2!=2'");
    assert!(results.contains(&"3!=6".to_string()), "Should find '3!=6'");
}

/// Test that factorial with greater-than operator is enumerated
#[test]
fn test_factorial_equations_with_greater_than_no_constraints() {
    let gk = empty_gk(4);
    let solver = Solver::new(4, gk);
    let (results, _searched) = solver.solve();

    // 3! > N patterns: 3! = 6, so 6 > 0..5
    assert!(results.contains(&"3!>0".to_string()), "Should find '3!>0'");
    assert!(results.contains(&"3!>5".to_string()), "Should find '3!>5'");

    // N > M! patterns: e.g., 2 > 0! (= 2 > 1), 2 > 1! (= 2 > 1)
    assert!(results.contains(&"2>0!".to_string()), "Should find '2>0!'");
    assert!(results.contains(&"2>1!".to_string()), "Should find '2>1!'");
}

/// Test that longer factorial equations are properly enumerated
#[test]
fn test_factorial_equations_length_5_no_constraints() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();

    // 4!=24 (4! = 24)
    assert!(
        results.contains(&"4!=24".to_string()),
        "Should find '4!=24'"
    );

    // Factorial combined with arithmetic: 1!*N=M, 1!+N=M, etc.
    let factorial_eq_solutions: Vec<_> = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('='))
        .collect();
    assert!(
        !factorial_eq_solutions.is_empty(),
        "Should find factorial equations with '=' in length 5"
    );
}

/// Test that factorial equations are enumerated in length 6
#[test]
fn test_factorial_equations_length_6_no_constraints() {
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    // 5!=120 (5! = 120)
    assert!(
        results.contains(&"5!=120".to_string()),
        "Should find '5!=120'"
    );

    // Various factorial arithmetic combinations
    let factorial_eq_solutions: Vec<_> = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('='))
        .collect();
    assert!(
        factorial_eq_solutions.len() >= 100,
        "Should find at least 100 factorial equations with '=' in length 6, found {}",
        factorial_eq_solutions.len()
    );
}

/// Test that factorial followed by arithmetic operators works
#[test]
fn test_factorial_with_arithmetic_no_constraints() {
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    // 1!*N=M patterns (1! = 1, so 1*N = M)
    let factorial_mult: Vec<_> = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('*') && s.contains('='))
        .collect();
    assert!(
        !factorial_mult.is_empty(),
        "Should find factorial multiplication equations"
    );

    // 1!+N=M patterns (1! = 1, so 1+N = M)
    let factorial_add: Vec<_> = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('+') && s.contains('='))
        .collect();
    assert!(
        !factorial_add.is_empty(),
        "Should find factorial addition equations"
    );

    // 1!-N=M patterns
    let factorial_sub: Vec<_> = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('-') && s.contains('='))
        .collect();
    assert!(
        !factorial_sub.is_empty(),
        "Should find factorial subtraction equations"
    );
}

/// Test that factorial at end of expression works (e.g., N>M!)
#[test]
fn test_factorial_at_end_no_constraints() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();

    // N>M! patterns
    let factorial_end: Vec<_> = results.iter().filter(|s| s.ends_with('!')).collect();
    assert!(
        !factorial_end.is_empty(),
        "Should find solutions ending with '!'"
    );

    // Specific examples: 10>0! (10 > 1), 10>1! (10 > 1)
    assert!(
        results.contains(&"10>0!".to_string()),
        "Should find '10>0!'"
    );
}

/// Test factorial equations with constraints still work
#[test]
fn test_factorial_with_constraints() {
    // Constraint: first char is '3' (correct), no other significant constraints
    // Use a row where only position 0 has a Correct tile,
    // and other positions have distinct chars that won't affect factorial
    let row1: GuessRow = vec![
        Tile {
            char: '3',
            state: TileState::Correct,
        },
        Tile {
            char: '0',
            state: TileState::Empty,
        }, // '0' not at pos 1
        Tile {
            char: '0',
            state: TileState::Empty,
        }, // '0' not at pos 2
        Tile {
            char: '0',
            state: TileState::Empty,
        }, // '0' not at pos 3
    ];
    let gk = GlobalKnowledge::from_guess_rows(4, &[row1]).unwrap();
    let solver = Solver::new(4, gk);
    let (results, _searched) = solver.solve();

    assert!(
        results.contains(&"3!=6".to_string()),
        "Should find '3!=6' with constraint fixing '3' at pos 0"
    );
}

/// Test that factorial with permutation (A) operator works
#[test]
fn test_factorial_with_permutation_no_constraints() {
    // Use length 6 which is reasonably fast
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _searched) = solver.solve();

    // Check factorial and permutation coexist in solutions
    let with_bang: Vec<_> = results.iter().filter(|s| s.contains('!')).collect();
    let with_a: Vec<_> = results.iter().filter(|s| s.contains('A')).collect();
    assert!(!with_bang.is_empty(), "Should have factorial solutions");
    assert!(!with_a.is_empty(), "Should have permutation solutions");
}

/// Test parallel solver also finds factorial equations
#[test]
fn test_parallel_solver_finds_factorial_equations() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk);
    let parallel_solver = ParallelSolver::new(solver, Some(2));
    let (results, _searched) = parallel_solver.solve();

    assert!(
        results.contains(&"4!=24".to_string()),
        "Parallel solver should find '4!=24'"
    );

    let factorial_eq_count = results
        .iter()
        .filter(|s| s.contains('!') && s.contains('='))
        .count();
    assert!(
        factorial_eq_count > 0,
        "Parallel solver should find factorial equations with '='"
    );
}

/// Test that factorial results are consistent between sequential and parallel solvers
#[test]
fn test_factorial_sequential_parallel_consistency() {
    let gk = empty_gk(5);
    let solver = Solver::new(5, gk.clone());

    let (seq_results, _) = solver.solve();

    let solver2 = Solver::new(5, gk);
    let parallel_solver = ParallelSolver::new(solver2, Some(2));
    let (par_results, _) = parallel_solver.solve();

    let mut seq_sorted = seq_results;
    seq_sorted.sort();
    let mut par_sorted = par_results;
    par_sorted.sort();

    assert_eq!(
        seq_sorted, par_sorted,
        "Sequential and parallel factorial results should match"
    );

    // Both should contain factorial equations
    let seq_factorial: Vec<_> = seq_sorted.iter().filter(|s| s.contains('!')).collect();
    assert!(
        !seq_factorial.is_empty(),
        "Both solvers should find factorial solutions"
    );
}

/// Verify that all factorial solutions are valid equations
#[test]
fn test_all_factorial_solutions_are_valid() {
    for length in 4..=8 {
        let gk = empty_gk(length);
        let solver = Solver::new(length, gk);
        let (results, _searched) = solver.solve();

        let factorial_solutions: Vec<_> = results.iter().filter(|s| s.contains('!')).collect();

        // Assert non-empty to prevent silent passes if a bug causes
        // the solver to return zero factorial solutions.
        assert!(
            !factorial_solutions.is_empty(),
            "Length {}: expected at least one factorial solution, but found none",
            length
        );

        for sol in &factorial_solutions {
            assert!(
                is_valid_equation(sol),
                "Factorial solution '{}' should be a valid equation",
                sol
            );
            assert_eq!(
                sol.len(),
                length,
                "Factorial solution '{}' should have length {}",
                sol,
                length
            );
        }
    }
}

/// Verify that close brackets can follow factorial operators (Gemini fix #1).
/// Before the fix, can_place_char unconditionally rejected ')' after any
/// operator, preventing valid equations like (3!)=6.
#[test]
fn test_factorial_close_bracket_pruning() {
    // Verify that expressions like (3!)=6 are generated
    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _) = solver.solve();

    // (3!)=6 should be found if factorial-close-bracket pruning is correct
    assert!(
        results.contains(&"(3!)=6".to_string()),
        "Should find '(3!)=6' — close bracket after factorial must be allowed"
    );

    // Also verify length-7 solutions contain (4!)=24
    let gk7 = empty_gk(7);
    let solver7 = Solver::new(7, gk7);
    let (results7, _) = solver7.solve();
    assert!(
        results7.contains(&"(4!)=24".to_string()),
        "Should find '(4!)=24' — close bracket after factorial must be allowed"
    );

    // Check that we find factorial-in-parentheses solutions in general
    let paren_factorial: Vec<_> = results.iter().filter(|s| s.contains("!)")).collect();
    assert!(
        !paren_factorial.is_empty(),
        "Should find at least one solution with ')' after '!'"
    );
}

/// The reference evaluator accepts `-0` as a simple numeric RHS.  Solver-side
/// pruning must therefore keep equations whose right side is exactly `-0`.
#[test]
fn test_solver_preserves_negative_zero_rhs() {
    assert!(is_valid_equation("1-1=-0"));

    let gk = empty_gk(6);
    let solver = Solver::new(6, gk);
    let (results, _) = solver.solve();
    assert!(
        results.contains(&"1-1=-0".to_string()),
        "Solver pruning must not drop valid negative-zero RHS equations"
    );
}

// =========================================================================
// Branch partitioning / high-thread consistency (regression for the reported
// "lost solutions" concern: a high thread count deepens the prefix and grows
// the eager `=` result set, which must stay a superset — never lossy).
// =========================================================================

/// Sorted solution set from a fresh solver of the given length and thread count.
fn solve_sorted(length: usize, threads: usize) -> (Vec<String>, u64) {
    let solver = Solver::new(length, empty_gk(length));
    let (mut results, searched) = if threads == 1 {
        solver.solve()
    } else {
        ParallelSolver::new(solver, Some(threads)).solve()
    };
    results.sort();
    (results, searched)
}

#[test]
fn test_high_thread_count_does_not_lose_solutions() {
    // A large thread count forces the branch prefix to deepen well past the
    // first level, so eager `=` solutions are collected at several depths.
    for length in 5..=7 {
        let (seq, seq_searched) = solve_sorted(length, 1);
        // 256 threads => branch target 4096 => deep prefix on small lengths.
        let (par, par_searched) = solve_sorted(length, 256);
        assert_eq!(
            seq, par,
            "length {length}: high-thread solution set must equal single-threaded"
        );
        assert_eq!(
            seq_searched, par_searched,
            "length {length}: searched count must match single-threaded"
        );
    }
}

// =========================================================================
// Streaming output consistency: solutions streamed via solve_to_writer must
// be exactly the default solution set (order aside).
// =========================================================================

#[test]
fn test_streaming_output_matches_default_set() {
    let length = 6;
    let (expected, _) = solve_sorted(length, 1);

    let solver = Solver::new(length, empty_gk(length));
    let ps = ParallelSolver::new(solver, Some(4));

    let tmp = std::env::temp_dir().join("sumzle_stream_test.jsonl");
    let file = std::io::BufWriter::new(std::fs::File::create(&tmp).unwrap());
    let never = std::sync::atomic::AtomicBool::new(false);
    let (written, _searched) = ps.solve_to_writer(file, &never).unwrap();

    let content = std::fs::read_to_string(&tmp).unwrap();
    let mut got: Vec<String> = content
        .lines()
        .map(|l| {
            // line form: {"solution":"<expr>"}
            let start = l.find(":\"").unwrap() + 2;
            let end = l.rfind("\"}").unwrap();
            l[start..end].to_string()
        })
        .collect();
    got.sort();
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(got, expected, "streamed set must equal default set");
    assert_eq!(
        written as usize,
        expected.len(),
        "streamed solution count must equal default count"
    );
}

// =========================================================================
// top-N consistency: scoring matches server::compute_recommended, and a large
// N returns the entire solution set.
// =========================================================================

#[test]
fn test_top_n_large_n_equals_full_set() {
    let length = 6;
    let (expected, _) = solve_sorted(length, 1);

    let solver = Solver::new(length, empty_gk(length));
    let ps = ParallelSolver::new(solver, Some(4));
    let (scored, _searched) = ps.solve_top_n(expected.len() + 10);

    let mut got: Vec<String> = scored.into_iter().map(|(_, s)| s).collect();
    got.sort();
    assert_eq!(
        got, expected,
        "top-N with N >= total must return the full solution set"
    );
}

#[test]
fn test_top_n_best_matches_compute_recommended() {
    use sumzle_solver::api::{compute_char_probabilities, compute_recommended};

    let length = 6;

    // Reference: full in-memory solve + server scoring.
    let solver = Solver::new(length, empty_gk(length));
    let (full, _) = solver.solve();
    let probs = compute_char_probabilities(&full);
    let recommended = compute_recommended(&full, &probs).unwrap();

    // All solutions with their top-N score (N >= total).
    let solver = Solver::new(length, empty_gk(length));
    let ps = ParallelSolver::new(solver, Some(4));
    let (scored, _) = ps.solve_top_n(full.len() + 10);

    // `compute_recommended` and `solve_top_n` may pick different solutions when
    // several tie for the maximum score (the former keeps the first in solve
    // order; the latter is deterministic). The meaningful invariant is that
    // both select a *maximum-scoring* solution, i.e. the scores agree.
    let max_score = scored
        .iter()
        .map(|(s, _)| *s)
        .fold(f64::NEG_INFINITY, f64::max);
    let rec_score = scored
        .iter()
        .find(|(_, sol)| *sol == recommended)
        .map(|(s, _)| *s)
        .expect("recommended solution must be present in the scored set");

    assert_eq!(
        rec_score, max_score,
        "server::compute_recommended must select a maximum-scoring solution"
    );

    // top-1 must also be a maximum-scoring solution with that same score.
    let (top1, _) = ps.solve_top_n(1);
    assert_eq!(top1.len(), 1, "top-1 returns exactly one solution");
    assert_eq!(
        top1[0].0, max_score,
        "top-1 must equal the maximum score over the full set"
    );
}

#[test]
fn test_top_n_output_ordering_and_tie_break() {
    // The documented contract: results are sorted by score descending, ties
    // broken by expression ascending; and when a score tie straddles the N
    // boundary, the lexicographically smaller expressions are the ones kept.
    let length = 6;
    let solver = Solver::new(length, empty_gk(length));
    let ps = ParallelSolver::new(solver, Some(4));
    let (scored, _) = ps.solve_top_n(50);

    // Ordering: (score desc, expr asc).
    for w in scored.windows(2) {
        let (sa, ea) = (&w[0].0, &w[0].1);
        let (sb, eb) = (&w[1].0, &w[1].1);
        assert!(
            sa > sb || (sa == sb && ea < eb),
            "top-N must be ordered by score desc then expr asc: ({sa},{ea}) before ({sb},{eb})"
        );
    }

    // Boundary keep-behavior: take a score that has more occurrences than the
    // slots we allow, and confirm we keep the lexicographically smallest exprs.
    let (full_scored, _) = {
        let solver = Solver::new(length, empty_gk(length));
        let ps = ParallelSolver::new(solver, Some(4));
        ps.solve_top_n(usize::MAX >> 8) // effectively "all", bounded sanity
    };
    // Group all solutions by score; find the maximum score's tie group.
    let max_score = full_scored
        .iter()
        .map(|(s, _)| *s)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut tied: Vec<String> = full_scored
        .iter()
        .filter(|(s, _)| *s == max_score)
        .map(|(_, e)| e.clone())
        .collect();
    tied.sort();

    if tied.len() >= 2 {
        // Ask for exactly one fewer than the tie-group size at the top: the
        // kept set must be the smallest exprs of the group.
        let k = tied.len() - 1;
        let solver = Solver::new(length, empty_gk(length));
        let ps = ParallelSolver::new(solver, Some(4));
        let (topk, _) = ps.solve_top_n(k);
        let kept: Vec<String> = topk.into_iter().map(|(_, e)| e).collect();
        assert_eq!(
            kept,
            tied[..k],
            "on a boundary score tie, the lexicographically smallest expressions must be kept"
        );
    }
}

// ---- Regression tests for PR review fixes ----

#[test]
fn invalid_guess_char_does_not_panic() {
    // A character outside the Sumzle charset in a guess row must not crash the
    // constraint-preparation path: `idx_of_char` returns `None` and the char is
    // dropped, instead of hitting `unreachable!` (which crashed the API/CLI).
    let row: GuessRow = vec![Tile {
        char: 'x',
        state: TileState::Empty,
    }];
    let gk = GlobalKnowledge::from_guess_rows(5, &[row]).expect("constraints should build");
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();
    // The unrepresentable char contributes no constraint, so the full
    // unconstrained length-5 solution set remains.
    assert_eq!(results.len(), 6243);
}

#[test]
fn floor_value_saturating_below_i64_min_does_not_panic() {
    // `[0-9^20]` floors to ~-1.2e19, below i64::MIN; the cast saturates and the
    // decimal formatter must not panic (previously `-i64::MIN` overflowed in
    // debug builds). Asserting it returns at all is enough to guard the panic.
    let _ = evaluate_expression("[0-9^20]");
}

#[test]
fn non_finite_numeric_rhs_is_rejected() {
    // A numeric RHS long enough to overflow f64 to infinity must be rejected,
    // not silently accepted because ±∞ and a large LHS both saturate to i64::MAX.
    let huge = "9".repeat(320);
    assert!(!is_valid_equation(&format!("9^20={huge}")));
}

#[test]
fn correct_invalid_guess_char_does_not_panic() {
    // A `Correct` tile fixes an exact character at a position. When that
    // character is outside the Sumzle charset (e.g. `x`), its raw byte must not
    // reach the search: `idx_of` would map it to `INVALID_INDEX` (255) and index
    // `char_counts` out of bounds (a DoS on the API/CLI). The fixed constraint is
    // dropped during preparation. The companion `cannot_be_at` set still forbids
    // every valid char at that position, so no solution can be placed there.
    let gk = gk_from_row(5, &[('x', TileState::Correct)]);
    let solver = Solver::new(5, gk);
    let (results, _searched) = solver.solve();
    assert!(
        results.is_empty(),
        "position fixed to an unrepresentable char admits no solutions"
    );
}

#[test]
fn solve_branch_invalid_first_char_does_not_panic() {
    // `solve_branch` is public and may be called with an arbitrary `first_char`.
    // A character outside the Sumzle charset must be rejected before it reaches
    // `idx_of`, which would otherwise index `char_counts` out of bounds.
    let solver = Solver::new(5, empty_gk(5));
    let (results, searched) = solver.solve_branch('x', None, FloorContext::new());
    assert!(results.is_empty());
    assert_eq!(searched, 0);
}

// =========================================================================
// RHS value index (issue: extremely large lengths).
//
// The index resolves the `>` operator's right-hand side with a binary search
// over a precomputed, value-sorted table instead of re-enumerating that
// subtree for every left-hand-side prefix. It is a pure accelerator: results
// must be bit-for-bit identical to the recursive search, including
// `searched_count`, and its memory must stay inside the caller's budget at any
// length. `memory_budget = 0` disables it and is used here as the reference
// engine.
// =========================================================================

/// Sorted solutions plus searched count from the pure recursive search.
fn solve_unindexed(length: usize) -> (Vec<String>, u64) {
    let (mut r, s) = Solver::with_memory_budget(length, empty_gk(length), 0).solve();
    r.sort();
    (r, s)
}

#[test]
fn test_rhs_index_matches_unindexed_search() {
    for length in 3..=8 {
        let (expected, expected_searched) = solve_unindexed(length);

        let (mut got, got_searched) = Solver::new(length, empty_gk(length)).solve();
        got.sort();

        assert_eq!(
            got, expected,
            "length {length}: indexed solution set must equal the recursive search"
        );
        assert_eq!(
            got_searched, expected_searched,
            "length {length}: indexed searched_count must equal the recursive search"
        );
    }
}

#[test]
fn test_rhs_index_matches_unindexed_in_parallel() {
    // The index is consulted both mid-search and when a worker resumes a
    // branch parked on the main operator; several thread counts exercise both.
    for length in 5..=7 {
        let (expected, expected_searched) = solve_unindexed(length);

        for threads in [1usize, 2, 4, 256] {
            let ps = ParallelSolver::new(Solver::new(length, empty_gk(length)), Some(threads));
            let (mut got, got_searched) = ps.solve();
            got.sort();

            assert_eq!(
                got, expected,
                "length {length}, {threads} threads: indexed set must match the recursive search"
            );
            assert_eq!(
                got_searched, expected_searched,
                "length {length}, {threads} threads: searched_count must match"
            );
        }
    }
}

#[test]
fn test_rhs_index_char_counts_match_unindexed() {
    // `CountSink` folds a whole index range from block prefix sums rather than
    // visiting each solution, so its per-character totals need their own check:
    // top-N's character probabilities are computed from exactly these numbers.
    use sumzle_solver::solver::CountSink;

    for length in 3..=7 {
        let mut expected = CountSink::new();
        let expected_searched =
            Solver::with_memory_budget(length, empty_gk(length), 0).solve_into(&mut expected);

        let mut got = CountSink::new();
        let got_searched = Solver::new(length, empty_gk(length)).solve_into(&mut got);

        assert_eq!(
            got.total, expected.total,
            "length {length}: total solutions"
        );
        assert_eq!(
            got.char_counts, expected.char_counts,
            "length {length}: per-character solution counts"
        );
        assert_eq!(got_searched, expected_searched, "length {length}: searched");
    }
}

#[test]
fn test_rhs_index_top_n_matches_unindexed() {
    // Top-N over an index range uses OR-tree branch-and-bound to skip whole
    // subtrees. Pruning must never drop a solution that belongs in the result,
    // so the ranking has to match the unaccelerated engine exactly.
    for length in 5..=7 {
        for n in [1usize, 5, 50] {
            let expected = ParallelSolver::new(
                Solver::with_memory_budget(length, empty_gk(length), 0),
                Some(2),
            )
            .solve_top_n(n);

            let got =
                ParallelSolver::new(Solver::new(length, empty_gk(length)), Some(2)).solve_top_n(n);

            assert_eq!(
                got.0.len(),
                expected.0.len(),
                "length {length}, top {n}: result size"
            );
            for (g, e) in got.0.iter().zip(expected.0.iter()) {
                assert_eq!(g.1, e.1, "length {length}, top {n}: ranked solution");
                assert!(
                    (g.0 - e.0).abs() < 1e-9,
                    "length {length}, top {n}: score for {} ({} vs {})",
                    g.1,
                    g.0,
                    e.0
                );
            }
            assert_eq!(
                got.1, expected.1,
                "length {length}, top {n}: searched count"
            );
        }
    }
}

#[test]
fn test_rhs_index_respects_memory_budget() {
    // The budget is a hard ceiling at every length: whatever does not fit is
    // simply not built, and the search falls back to recursion. This is what
    // keeps memory bounded for arbitrarily long puzzles.
    for length in [6usize, 8, 10, 12, 14] {
        for budget_mb in [0usize, 1, 16, 64] {
            let budget = budget_mb * 1024 * 1024;
            let solver = Solver::with_memory_budget(length, empty_gk(length), budget);
            assert!(
                solver.index_bytes() <= budget,
                "length {length}: index used {} bytes, over the {budget}-byte budget",
                solver.index_bytes()
            );
        }
    }
}

/// Peak RSS is what a memory budget actually has to bound — a build that ends
/// up inside the budget is still a failure if it spikes far past it on the way
/// there (that spike is what makes a long puzzle die on a small host).
///
/// Runs in a **child process** so the measurement is not polluted by the rest
/// of the suite: `VmHWM` is a process-wide high-water mark that never resets,
/// so a peak set by a concurrently running test would be indistinguishable
/// from one set here. The child re-executes this binary with a marker
/// environment variable and reports its own peak.
#[cfg(target_os = "linux")]
#[test]
fn test_rhs_index_build_peak_stays_near_budget() {
    const MARKER: &str = "SUMZLE_RHS_INDEX_PEAK_CHILD";
    // Long enough that an unbounded index would be enormous, so the budget is
    // certainly the binding constraint.
    const BUDGET_MB: usize = 32;
    const LENGTH: usize = 14;

    fn peak_rss_bytes() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .expect("read /proc/self/status")
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse::<usize>().ok())
            .expect("VmHWM field")
            * 1024
    }

    // Child role: build the index and print the two numbers the parent checks.
    if std::env::var_os(MARKER).is_some() {
        let budget = BUDGET_MB * 1024 * 1024;
        let solver = Solver::with_memory_budget(LENGTH, empty_gk(LENGTH), budget);
        println!("INDEX_BYTES={}", solver.index_bytes());
        println!("PEAK_BYTES={}", peak_rss_bytes());
        return;
    }

    let exe = std::env::current_exe().expect("current test binary");
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "test_rhs_index_build_peak_stays_near_budget",
            "--nocapture",
        ])
        .env(MARKER, "1")
        .output()
        .expect("re-run this test binary as a child");
    assert!(
        out.status.success(),
        "child process failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let field = |name: &str| -> usize {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("child did not report {name}; output:\n{stdout}"))
    };
    let index_bytes = field("INDEX_BYTES=");
    let peak = field("PEAK_BYTES=");

    let budget = BUDGET_MB * 1024 * 1024;
    assert!(
        index_bytes <= budget,
        "finished index of {index_bytes} bytes exceeds the {budget}-byte budget"
    );
    // Generous headroom over the budget: this guards against the peak scaling
    // with the *puzzle*, it is not a tight allocator accounting check. The
    // child's peak also includes the test binary's own baseline footprint.
    let limit = budget * 6;
    assert!(
        peak < limit,
        "peak RSS reached {} MB while building a {BUDGET_MB} MB index at length \
         {LENGTH} — the build-time spike is not bounded by the budget",
        peak / (1024 * 1024),
    );
}

#[test]
fn test_rhs_index_memory_does_not_grow_with_length() {
    // The property the bounded modes exist for: past the point where the
    // budget binds, making the puzzle longer must not make the index bigger.
    const BUDGET: usize = 16 * 1024 * 1024;
    let at = |len: usize| Solver::with_memory_budget(len, empty_gk(len), BUDGET).index_bytes();

    let baseline = at(12);
    for length in [13usize, 16, 20] {
        assert_eq!(
            at(length),
            baseline,
            "index size must plateau once the budget binds (length {length})"
        );
    }
}

#[test]
fn test_rhs_index_tiny_budget_still_correct() {
    // A budget that fits only the shortest right-hand sides leaves the rest on
    // the recursive path, so both engines run within one solve. The mixture
    // must still produce exactly the reference result.
    for length in 5..=7 {
        let (expected, expected_searched) = solve_unindexed(length);

        // 4 KiB: enough for k=1, far too small for the longer tables.
        let (mut got, got_searched) =
            Solver::with_memory_budget(length, empty_gk(length), 4096).solve();
        got.sort();

        assert_eq!(got, expected, "length {length}: partial-index solution set");
        assert_eq!(
            got_searched, expected_searched,
            "length {length}: partial-index searched_count"
        );
    }
}

#[test]
fn test_rhs_index_with_constraints_matches_unindexed() {
    // Positional constraints are safe to bake into the index (an RHS
    // character's absolute position is fixed once the operator position is),
    // while count constraints are not and must disable it. Both kinds are
    // checked against the reference engine.
    let length = 6;

    let cases: Vec<(&str, Vec<Tile>)> = vec![
        (
            "fixed and absent",
            vec![
                Tile {
                    char: '1',
                    state: TileState::Correct,
                },
                Tile {
                    char: '+',
                    state: TileState::Present,
                },
                Tile {
                    char: '2',
                    state: TileState::Empty,
                },
                Tile {
                    char: '=',
                    state: TileState::Correct,
                },
                Tile {
                    char: '3',
                    state: TileState::Empty,
                },
                Tile {
                    char: '0',
                    state: TileState::Empty,
                },
            ],
        ),
        (
            "present forces count constraints",
            vec![
                Tile {
                    char: '5',
                    state: TileState::Present,
                },
                Tile {
                    char: '>',
                    state: TileState::Present,
                },
                Tile {
                    char: '7',
                    state: TileState::Empty,
                },
                Tile {
                    char: '1',
                    state: TileState::Present,
                },
                Tile {
                    char: '4',
                    state: TileState::Empty,
                },
                Tile {
                    char: '9',
                    state: TileState::Empty,
                },
            ],
        ),
    ];

    for (name, row) in cases {
        let gk = GlobalKnowledge::from_guess_rows(length, std::slice::from_ref(&row)).unwrap();

        let (mut expected, expected_searched) =
            Solver::with_memory_budget(length, gk.clone(), 0).solve();
        expected.sort();

        let (mut got, got_searched) = Solver::new(length, gk).solve();
        got.sort();

        assert_eq!(got, expected, "{name}: solution set must match");
        assert_eq!(got_searched, expected_searched, "{name}: searched_count");
    }
}

#[test]
fn test_rhs_index_streaming_matches_unindexed() {
    // The streaming sink takes the default `accept_index_range`, which expands
    // a range entry by entry; this pins that expansion against the reference.
    let length = 6;
    let (expected, _) = solve_unindexed(length);

    let tmp = std::env::temp_dir().join("sumzle_rhs_index_stream_test.jsonl");
    let file = std::io::BufWriter::new(std::fs::File::create(&tmp).unwrap());
    let never = std::sync::atomic::AtomicBool::new(false);
    let ps = ParallelSolver::new(Solver::new(length, empty_gk(length)), Some(4));
    let (written, _searched) = ps.solve_to_writer(file, &never).unwrap();

    let content = std::fs::read_to_string(&tmp).unwrap();
    let mut got: Vec<String> = content
        .lines()
        .map(|l| {
            let start = l.find(":\"").unwrap() + 2;
            let end = l.rfind("\"}").unwrap();
            l[start..end].to_string()
        })
        .collect();
    got.sort();
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(
        got, expected,
        "streamed set must match the recursive search"
    );
    assert_eq!(written as usize, expected.len(), "streamed count");
}

// =========================================================================
// Bounded (approximate) top-N.
//
// The search space is exponential in the puzzle length, so past a point no
// exhaustive search finishes. A `SearchLimit` makes top-N return the best
// ranking it found within a budget, flagged as approximate. The contract:
// an unlimited search is still exact; a limited one stops, says so, and
// still returns usable solutions.
// =========================================================================

#[test]
fn test_unlimited_search_reports_complete_and_stays_exact() {
    use sumzle_solver::limit::SearchLimit;
    use sumzle_solver::parallel::Progress;

    let length = 6;
    let reference = ParallelSolver::new(
        Solver::with_memory_budget(length, empty_gk(length), 0),
        Some(2),
    )
    .solve_top_n(10);

    let ps = ParallelSolver::new(Solver::new(length, empty_gk(length)), Some(2));
    let (scored, _counts, searched, complete) =
        ps.solve_top_n_limited(10, &Progress::new(), &SearchLimit::unlimited());

    assert!(complete, "an unlimited search must report completion");
    assert_eq!(searched, reference.1, "searched count must be exact");
    let got: Vec<&str> = scored.iter().map(|(_, s)| s.as_str()).collect();
    let want: Vec<&str> = reference.0.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(got, want, "an unlimited search must be exact");
}

#[test]
fn test_limited_search_reports_incomplete_but_still_ranks() {
    use sumzle_solver::limit::SearchLimit;
    use sumzle_solver::parallel::Progress;

    // A budget far below the ~14M expressions a length-8 solve needs, so the
    // search certainly stops early.
    let length = 8;
    let ps = ParallelSolver::new(Solver::new(length, empty_gk(length)), Some(2));
    let (scored, _counts, _searched, complete) =
        ps.solve_top_n_limited(5, &Progress::new(), &SearchLimit::with_max_searched(50_000));

    assert!(!complete, "a truncated search must report incompleteness");
    // The point of the feature: a partial search still answers. Every returned
    // solution must be a real one, and the ranking must be ordered.
    assert!(
        !scored.is_empty(),
        "a truncated top-N must still return solutions, not an empty result"
    );
    assert!(scored.len() <= 5);
    for (_, sol) in &scored {
        assert_eq!(sol.len(), length, "solution has the requested length");
        assert!(
            is_valid_equation(sol),
            "approximate results must still be genuine solutions: {sol}"
        );
    }
    for w in scored.windows(2) {
        assert!(w[0].0 >= w[1].0, "scores must be sorted descending");
    }
}

#[test]
fn test_limit_split_leaves_room_for_the_scoring_pass() {
    use sumzle_solver::limit::SearchLimit;

    // Regression: top-N makes two passes over the same space. When they shared
    // a single budget, pass 1 spent all of it and pass 2 ranked nothing, so a
    // budgeted solve returned zero solutions — strictly worse than an
    // approximate answer. Each pass now gets its own share.
    //
    // The cap is large enough that halving it stays above the CHECK_INTERVAL
    // floor, so this exercises real proportional splitting rather than the
    // floor.
    let total = sumzle_solver::limit::CHECK_INTERVAL * 100;
    let limit = SearchLimit::with_max_searched(total);

    let first = limit.split(0.5);
    assert!(
        first.charge(total),
        "a charge of the whole cap exhausts the first pass's half"
    );
    assert!(first.is_exceeded());

    // Pass 1 ran out, so the overall result is approximate...
    limit.absorb(&first);
    assert!(
        limit.stopped_early(),
        "a truncated pass must mark the whole solve approximate"
    );

    // ...but pass 2 still gets an allowance, so it can produce a ranking.
    let second = limit.split(1.0);
    assert!(
        !second.is_exceeded(),
        "the second pass must start with budget of its own"
    );
    assert!(!second.charge(1), "and must be able to do real work");
}

#[test]
fn test_cancelled_limit_is_not_resurrected_by_split() {
    use sumzle_solver::limit::SearchLimit;

    // Cancellation (client disconnected) must survive the pass split —
    // otherwise pass 2 would happily start searching for a caller that is
    // already gone.
    let limit = SearchLimit::unlimited();
    limit.cancel();
    assert!(limit.split(1.0).is_exceeded());
}

#[cfg(target_os = "linux")]
#[test]
fn test_extreme_length_top_n_stays_bounded() {
    use std::time::{Duration, Instant};
    use sumzle_solver::limit::SearchLimit;
    use sumzle_solver::parallel::Progress;

    // The headline capability: a length far past anything an exhaustive search
    // could enumerate still answers, in bounded time, without exhausting
    // memory. 32 MiB index budget keeps this cheap enough for CI.
    const LENGTH: usize = 18;
    let solver = Solver::with_memory_budget(LENGTH, empty_gk(LENGTH), 32 * 1024 * 1024);
    let ps = ParallelSolver::new(solver, Some(2));

    let started = Instant::now();
    let (scored, _counts, _searched, complete) = ps.solve_top_n_limited(
        3,
        &Progress::new(),
        &SearchLimit::with_timeout(Duration::from_secs(5)),
    );
    let elapsed = started.elapsed();

    assert!(!complete, "length {LENGTH} cannot be searched exhaustively");
    assert!(
        elapsed < Duration::from_secs(60),
        "a 5s budget must be respected (took {elapsed:?})"
    );
    assert!(
        !scored.is_empty(),
        "must return an approximate ranking at length {LENGTH}"
    );
    for (_, sol) in &scored {
        assert_eq!(sol.len(), LENGTH);
        assert!(
            is_valid_equation(sol),
            "approximate result must be a genuine solution: {sol}"
        );
    }
}
