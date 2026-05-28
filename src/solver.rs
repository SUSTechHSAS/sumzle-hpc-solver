//! Brute-force search solver with pruning for Sumzle

use crate::evaluator::{check_brackets, is_valid_equation};
use crate::types::*;
use std::collections::HashMap;

/// Get optimized character order for a given position and context
fn get_optimized_char_order(
    index: usize,
    expr: &[Option<char>],
    length: usize,
    main_op_so_far: Option<char>,
    floor_ctx: FloorContext,
    gk: &GlobalKnowledge,
) -> Vec<char> {
    if let Some(fixed) = gk.fixed_chars[index] {
        return vec![fixed];
    }

    let prev_char = if index > 0 { expr[index - 1] } else { None };

    let ordered = if floor_ctx.in_floor {
        if floor_ctx.has_slash_in_current_floor {
            vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ']']
        } else {
            vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '/']
        }
    } else if main_op_so_far == Some('=') {
        if prev_char == Some('=') {
            vec!['-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9']
        } else {
            vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']
        }
    } else if index == 0 {
        vec!['1', '2', '3', '4', '5', '6', '7', '8', '9', '(', '[']
    } else if let Some(pc) = prev_char {
        if is_digit(pc) {
            vec![
                '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '+', '-', '*', '/', '%', '^',
                'A', '!', ')', ']', '[', '=', '>',
            ]
        } else if is_operator(pc) || is_open_bracket(pc) {
            vec!['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '(', '[']
        } else if is_close_bracket(pc) || is_unary_post_operator(pc) {
            vec![
                '+', '-', '*', '/', '%', '^', 'A', '!', ')', ']', '[', '=', '>',
            ]
        } else if is_main_operator(pc) {
            vec!['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '(', '[']
        } else {
            vec![
                '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '+', '-', '*', '/', '=', '(',
                '[', ')', ']', '%', '^', '!', 'A', '>',
            ]
        }
    } else {
        vec![
            '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '+', '-', '*', '/', '=', '(', '[',
            ')', ']', '%', '^', '!', 'A', '>',
        ]
    };

    // Filter for last position
    let ordered = if index == length - 1 && !floor_ctx.in_floor {
        let end_chars: &[char] = &[
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ')', ']', '!',
        ];
        let filtered: Vec<char> = ordered
            .iter()
            .filter(|c| end_chars.contains(c))
            .copied()
            .collect();
        if !filtered.is_empty() {
            filtered
        } else if prev_char.is_some() {
            end_chars.to_vec()
        } else if index == 0 && length == 1 {
            vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']
        } else {
            ordered
        }
    } else {
        ordered
    };

    // Deduplicate and filter by constraints
    let mut seen = std::collections::HashSet::new();
    ordered
        .into_iter()
        .filter(|c| seen.insert(*c))
        .filter(|c| !gk.globally_forbidden.contains(c) && !gk.cannot_be_at[index].contains(c))
        .collect()
}

/// Check if a character can be placed at a given position
#[allow(clippy::too_many_arguments)]
fn can_place_char(
    ch: char,
    index: usize,
    expr: &[Option<char>],
    main_op_so_far: Option<char>,
    char_counts: &HashMap<char, usize>,
    floor_ctx: FloorContext,
    gk: &GlobalKnowledge,
    length: usize,
) -> bool {
    // Global constraints
    if gk.globally_forbidden.contains(&ch) {
        return false;
    }
    if let Some(fixed) = gk.fixed_chars[index] {
        if fixed != ch {
            return false;
        }
    }
    if gk.cannot_be_at[index].contains(&ch) {
        return false;
    }

    // Exact count constraint
    if let Some(&exact) = gk.must_appear_exact_count.get(&ch) {
        let current = char_counts.get(&ch).copied().unwrap_or(0);
        if current >= exact {
            return false;
        }
    }

    // Floor context constraints
    if floor_ctx.in_floor {
        if ch == '[' {
            return false;
        }
        if is_operator(ch) && ch != '/' {
            return false;
        }
        if is_main_operator(ch) {
            return false;
        }
        if ch == '(' {
            return false;
        }
        if ch == 'A' || ch == '!' {
            return false;
        }

        if ch == '/' {
            if floor_ctx.has_slash_in_current_floor {
                return false;
            }
            let prev = if index > 0 { expr[index - 1] } else { None };
            if !prev.is_some_and(is_digit) || index == 0 {
                return false;
            }
        } else if ch == ']' {
            let prev = if index > 0 { expr[index - 1] } else { None };
            if !prev.is_some_and(is_digit) {
                return false;
            }
            if !floor_ctx.has_slash_in_current_floor {
                return false;
            }
        } else if !is_digit(ch) {
            return false;
        }
    }

    // Floor bracket constraints
    if ch == '[' && floor_ctx.in_floor {
        return false;
    }
    if ch == ']' && !floor_ctx.in_floor {
        return false;
    }
    if ch == '[' && index >= length - 3 {
        return false;
    }

    // Leading zero check and operand value check
    if is_digit(ch) && main_op_so_far != Some('=') {
        let mut temp_num_str = String::new();
        temp_num_str.push(ch);
        let mut k = index as i32 - 1;
        while k >= 0 {
            if let Some(Some(c)) = expr.get(k as usize) {
                if is_digit(*c) {
                    temp_num_str = format!("{}{}", c, temp_num_str);
                    k -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Leading zero check
        if temp_num_str.len() > 1 && temp_num_str.starts_with('0') {
            return false;
        }

        // Operand value check
        let char_before = if k >= 0 { expr[k as usize] } else { None };
        if char_before.is_none()
            || char_before.is_some_and(is_operator)
            || char_before.is_some_and(is_open_bracket)
            || char_before.is_some_and(is_main_operator)
        {
            if let Ok(val) = temp_num_str.parse::<i64>() {
                if val > MAX_OPERAND_VALUE {
                    return false;
                }
            }
        }
    }

    // First position rules
    if index == 0
        && (is_binary_operator(ch)
            || is_close_bracket(ch)
            || is_main_operator(ch)
            || is_unary_post_operator(ch))
    {
        return false;
    }

    // Previous character-based rules
    let prev_char = if index > 0 { expr[index - 1] } else { None };

    if let Some(pc) = prev_char {
        if is_digit(pc) {
            if is_open_bracket(ch) && ch != '[' {
                return false;
            }
            if ch == '[' && floor_ctx.in_floor {
                return false;
            }
        } else if is_operator(pc) {
            if is_binary_operator(ch)
                && !(pc == 'A' && (is_open_bracket(ch) || is_digit(ch)))
                && !is_unary_post_operator(pc)
            {
                return false;
            }
            if is_close_bracket(ch) {
                return false;
            }
            if is_main_operator(ch) && !is_unary_post_operator(pc) {
                return false;
            }
            if is_unary_post_operator(pc) && (is_digit(ch) || is_open_bracket(ch)) {
                return false;
            }
        } else if is_open_bracket(pc) {
            if pc == '[' && ch == '(' {
                return false;
            }
            if is_binary_operator(ch) {
                return false;
            }
            if is_close_bracket(ch) && get_matching_bracket(pc) != Some(ch) {
                return false;
            }
            if is_main_operator(ch) {
                return false;
            }
            if is_unary_post_operator(ch) {
                return false;
            }
        } else if is_close_bracket(pc) {
            if is_digit(ch) {
                return false;
            }
            if is_open_bracket(ch) {
                return false;
            }
        } else if is_main_operator(pc) {
            if pc == '=' {
                if !is_digit(ch) && ch != '-' {
                    return false;
                }
            } else if is_main_operator(ch) || is_close_bracket(ch) {
                return false;
            }
        }
    }

    // After main operator =, only digits and minus
    if main_op_so_far == Some('=') {
        if !is_digit(ch) && ch != '-' {
            return false;
        }
        if ch == '-' && prev_char == Some('=') && index >= length - 1 {
            return false;
        }
    }

    // Last position rules
    if index == length - 1
        && (is_binary_operator(ch) || is_open_bracket(ch) || is_main_operator(ch))
    {
        return false;
    }

    // Bracket balance check
    let mut temp_expr: Vec<char> = Vec::new();
    for i in 0..index {
        if let Some(Some(c)) = expr.get(i) {
            temp_expr.push(*c);
        }
    }
    temp_expr.push(ch);

    let mut open_paren_depth: i32 = 0;
    let mut open_square_depth: i32 = 0;
    let mut bracket_stack: Vec<char> = Vec::new();

    for &c in &temp_expr {
        if c == '(' {
            open_paren_depth += 1;
            bracket_stack.push(c);
        } else if c == '[' {
            open_square_depth += 1;
            bracket_stack.push(c);
        } else if c == ')' {
            open_paren_depth -= 1;
            if open_paren_depth < 0 || bracket_stack.pop() != Some('(') {
                return false;
            }
        } else if c == ']' {
            open_square_depth -= 1;
            if open_square_depth < 0 || bracket_stack.pop() != Some('[') {
                return false;
            }
        }
    }

    if index == length - 1 && (open_paren_depth != 0 || open_square_depth != 0) {
        return false;
    }

    // Main operator rules
    if is_main_operator(ch) {
        match main_op_so_far {
            None => {}
            Some(mop) => {
                if mop != ch && !(mop == '>' && ch == '=') {
                    return false;
                }
                if mop == ch && ch == '=' {
                    return false;
                }
            }
        }
        if index == 0 || index >= length - 1 {
            return false;
        }
    }

    // Permutation A rules
    if ch == 'A' && !prev_char.is_some_and(|pc| is_digit(pc) || is_close_bracket(pc)) {
        return false;
    }
    if prev_char == Some('A') && !is_digit(ch) && !is_open_bracket(ch) {
        return false;
    }

    // Factorial ! rules
    if ch == '!' {
        if prev_char.is_none() {
            return false;
        }
        if let Some(pc) = prev_char {
            // Only digit or ')' can precede '!'; ']' is not allowed
            if !is_digit(pc) && pc != ')' {
                return false;
            }
        }
    }

    true
}

/// Update floor context when placing a character
fn update_floor_context(ch: char, ctx: FloorContext) -> FloorContext {
    match ch {
        '[' => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: false,
        },
        ']' if ctx.in_floor => FloorContext {
            in_floor: false,
            has_slash_in_current_floor: false,
        },
        '/' if ctx.in_floor => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: true,
        },
        _ => ctx,
    }
}

/// The main solver struct
pub struct Solver {
    pub length: usize,
    pub gk: GlobalKnowledge,
}

impl Solver {
    pub fn new(length: usize, gk: GlobalKnowledge) -> Self {
        Self { length, gk }
    }

    /// Solve with single-threaded brute force
    pub fn solve(&self) -> (Vec<String>, u64) {
        let mut results: Vec<String> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<Option<char>> = vec![None; self.length];
        let mut char_counts: HashMap<char, usize> = HashMap::new();

        self.recursive_search(
            0,
            &mut expr,
            None,
            &mut char_counts,
            FloorContext::new(),
            &mut results,
            &mut searched_count,
        );

        (results, searched_count)
    }

    #[allow(clippy::too_many_arguments)]
    fn recursive_search(
        &self,
        index: usize,
        expr: &mut [Option<char>],
        main_op_so_far: Option<char>,
        char_counts: &mut HashMap<char, usize>,
        floor_ctx: FloorContext,
        results: &mut Vec<String>,
        searched_count: &mut u64,
    ) {
        if index == self.length {
            *searched_count += 1;

            // Must have a main operator
            if main_op_so_far.is_none() {
                return;
            }

            // Build expression string
            let expr_str: String = expr.iter().filter_map(|c| *c).collect();

            // Check brackets
            if !check_brackets(&expr_str) {
                return;
            }

            // Check exact count constraints
            for (&ch, &exact) in &self.gk.must_appear_exact_count {
                if char_counts.get(&ch).copied().unwrap_or(0) != exact {
                    return;
                }
            }

            // Check min count constraints (only for chars not in exact count)
            for (&ch, &min) in &self.gk.must_appear_min_count {
                if !self.gk.must_appear_exact_count.contains_key(&ch)
                    && char_counts.get(&ch).copied().unwrap_or(0) < min
                {
                    return;
                }
            }

            // Validate the equation
            if is_valid_equation(&expr_str) {
                results.push(expr_str);
            }
            return;
        }

        let fixed_char = self.gk.fixed_chars[index];

        if let Some(ch) = fixed_char {
            let next_floor_ctx = update_floor_context(ch, floor_ctx);

            if can_place_char(
                ch,
                index,
                expr,
                main_op_so_far,
                char_counts,
                floor_ctx,
                &self.gk,
                self.length,
            ) {
                expr[index] = Some(ch);
                *char_counts.entry(ch).or_insert(0) += 1;
                let new_main_op = if is_main_operator(ch) {
                    Some(ch)
                } else {
                    main_op_so_far
                };

                self.recursive_search(
                    index + 1,
                    expr,
                    new_main_op,
                    char_counts,
                    next_floor_ctx,
                    results,
                    searched_count,
                );

                let count = char_counts.get_mut(&ch).unwrap();
                *count -= 1;
                if *count == 0 {
                    char_counts.remove(&ch);
                }
            }
        } else {
            let chars_to_try = get_optimized_char_order(
                index,
                expr,
                self.length,
                main_op_so_far,
                floor_ctx,
                &self.gk,
            );

            for ch in chars_to_try {
                let next_floor_ctx = update_floor_context(ch, floor_ctx);

                if can_place_char(
                    ch,
                    index,
                    expr,
                    main_op_so_far,
                    char_counts,
                    floor_ctx,
                    &self.gk,
                    self.length,
                ) {
                    expr[index] = Some(ch);
                    *char_counts.entry(ch).or_insert(0) += 1;
                    let new_main_op = if is_main_operator(ch) {
                        Some(ch)
                    } else {
                        main_op_so_far
                    };

                    self.recursive_search(
                        index + 1,
                        expr,
                        new_main_op,
                        char_counts,
                        next_floor_ctx,
                        results,
                        searched_count,
                    );

                    let count = char_counts.get_mut(&ch).unwrap();
                    *count -= 1;
                    if *count == 0 {
                        char_counts.remove(&ch);
                    }
                }
            }
        }

        expr[index] = None;
    }

    /// Get the top-level character branches for parallel execution
    pub fn get_top_level_branches(&self) -> Vec<(char, Option<char>, FloorContext)> {
        let expr: Vec<Option<char>> = vec![None; self.length];
        let char_counts: HashMap<char, usize> = HashMap::new();

        let chars =
            get_optimized_char_order(0, &expr, self.length, None, FloorContext::new(), &self.gk);

        chars
            .into_iter()
            .filter(|&ch| {
                can_place_char(
                    ch,
                    0,
                    &expr,
                    None,
                    &char_counts,
                    FloorContext::new(),
                    &self.gk,
                    self.length,
                )
            })
            .map(|ch| {
                let main_op = if is_main_operator(ch) { Some(ch) } else { None };
                let floor_ctx = update_floor_context(ch, FloorContext::new());
                (ch, main_op, floor_ctx)
            })
            .collect()
    }

    /// Solve a single branch starting from a given first character
    pub fn solve_branch(
        &self,
        first_char: char,
        main_op: Option<char>,
        floor_ctx: FloorContext,
    ) -> (Vec<String>, u64) {
        let mut results: Vec<String> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<Option<char>> = vec![None; self.length];
        let mut char_counts: HashMap<char, usize> = HashMap::new();

        // Place the first character
        expr[0] = Some(first_char);
        *char_counts.entry(first_char).or_insert(0) += 1;

        self.recursive_search(
            1,
            &mut expr,
            main_op,
            &mut char_counts,
            floor_ctx,
            &mut results,
            &mut searched_count,
        );

        (results, searched_count)
    }
}
