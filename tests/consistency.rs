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
    for length in 3..=6 {
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
    for length in 4..=6 {
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
