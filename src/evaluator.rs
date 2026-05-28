//! Expression evaluator for Sumzle equations
//!
//! This evaluator closely follows the behavior of the reference JavaScript
//! implementation, using f64 internally for arithmetic and checking for
//! integer results at the equation validation level.

use crate::types::*;

/// Check if brackets are properly matched
pub fn check_brackets(expr: &str) -> bool {
    let mut stack: Vec<char> = Vec::new();
    for ch in expr.chars() {
        if is_open_bracket(ch) {
            stack.push(ch);
        } else if is_close_bracket(ch) {
            if stack.is_empty() {
                return false;
            }
            let last_open = stack.pop().unwrap();
            if get_matching_bracket(last_open) != Some(ch) {
                return false;
            }
        }
    }
    stack.is_empty()
}

/// Evaluate a mathematical expression, returning None if invalid.
/// Returns f64 to support fractional intermediate results (e.g., 7/2 = 3.5).
pub fn evaluate_expression(expr: &str) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut processed = expr.to_string();

    // Handle floor brackets [expr]
    // Replace [inner] with floor(eval(inner))
    let mut bracket_iterations = 0;
    const MAX_BRACKET_ITERATIONS: usize = 10;

    while bracket_iterations < MAX_BRACKET_ITERATIONS {
        // Find innermost [...]
        if let Some(start) = processed.rfind('[') {
            if let Some(end) = processed[start + 1..].find(']') {
                let inner_start = start + 1;
                let inner_end = start + 1 + end;
                let inner_expr = &processed[inner_start..inner_end];

                if inner_expr.is_empty() {
                    return None;
                }

                // Evaluate the inner expression
                let inner_val = evaluate_inner_expression(inner_expr)?;
                if !inner_val.is_finite() {
                    return None;
                }
                let floored = inner_val.floor() as i64;
                processed = format!(
                    "{}{}{}",
                    &processed[..start],
                    floored,
                    &processed[inner_end + 1..]
                );
            } else {
                return None;
            }
        } else {
            break;
        }
        bracket_iterations += 1;
    }

    if bracket_iterations >= MAX_BRACKET_ITERATIONS && processed.contains('[') {
        return None;
    }

    // Handle factorial: digits!
    processed = handle_factorials(&processed)?;

    // Handle permutation: digitsAdigits (like 5A3 = 5*4*3)
    processed = handle_permutations(&processed)?;

    // Now evaluate the simple arithmetic expression
    evaluate_arithmetic(&processed)
}

/// Evaluate an expression inside floor brackets (no nested brackets expected)
fn evaluate_inner_expression(expr: &str) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut processed = expr.to_string();

    // Handle factorial inside brackets
    processed = handle_factorials(&processed)?;
    // Handle permutation inside brackets
    processed = handle_permutations(&processed)?;

    evaluate_arithmetic(&processed)
}

/// Handle factorial expressions in the string
fn handle_factorials(expr: &str) -> Option<String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '!' {
            // Look back for the number in result
            let bs: Vec<char> = result.chars().collect();
            if bs.is_empty() {
                return None;
            }

            // Walk back to find the trailing number
            let mut j = bs.len();
            while j > 0 && bs[j - 1].is_ascii_digit() {
                j -= 1;
            }

            if j == bs.len() {
                // No number before !
                return None;
            }

            let num_str: String = bs[j..].iter().collect();
            let n: u64 = num_str.parse().ok()?;

            if n > MAX_FACTORIAL {
                return None;
            }

            let factorial = compute_factorial(n);

            // Replace the number part
            result = bs[..j].iter().collect();
            result.push_str(&factorial.to_string());
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }

    Some(result)
}

/// Compute factorial
fn compute_factorial(n: u64) -> u64 {
    if n == 0 {
        return 1;
    }
    let mut result: u64 = 1;
    for i in 2..=n {
        result *= i;
    }
    result
}

/// Handle permutation expressions (nAr = n!/(n-r)!)
fn handle_permutations(expr: &str) -> Option<String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == 'A' {
            // Look back for number before A
            let bs: Vec<char> = result.chars().collect();
            if bs.is_empty() {
                return None;
            }

            let mut j = bs.len();
            while j > 0 && bs[j - 1].is_ascii_digit() {
                j -= 1;
            }

            if j == bs.len() {
                // No number before A
                return None;
            }

            let m_str: String = bs[j..].iter().collect();
            let m: u64 = m_str.parse().ok()?;

            // Look ahead for number after A
            let mut k = i + 1;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }

            if k == i + 1 {
                // No number after A
                return None;
            }

            let n_str: String = chars[i + 1..k].iter().collect();
            let n: u64 = n_str.parse().ok()?;

            if m > MAX_PERMUTATION || n > MAX_PERMUTATION || n > m {
                return None;
            }

            let perm = compute_permutation(m, n);

            // Replace
            result = bs[..j].iter().collect();
            result.push_str(&perm.to_string());
            i = k; // Skip past the digits after A
            continue;
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }

    Some(result)
}

/// Compute permutation P(m,n) = m!/(m-n)!
fn compute_permutation(m: u64, n: u64) -> u64 {
    let mut result: u64 = 1;
    for i in 0..n {
        result *= m - i;
    }
    result
}

/// Evaluate a simple arithmetic expression using a recursive descent parser.
/// Supports: +, -, *, /, %, ^, parentheses
fn evaluate_arithmetic(expr: &str) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    // Check for leading zeros in multi-digit numbers
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            if chars[i] == '0' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                // This is a leading zero in a multi-digit number
                return None;
            }
            // Skip remaining digits of this number
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    // Validate: only allowed characters
    for ch in expr.chars() {
        if !ch.is_ascii_digit()
            && !matches!(ch, '+' | '-' | '*' | '/' | '%' | '^' | '(' | ')' | ' ')
        {
            return None;
        }
    }

    let mut parser = Parser::new(expr);
    let result = parser.parse_expression()?;

    // Ensure we consumed all input
    if parser.pos < parser.chars.len() {
        return None;
    }

    if result.is_nan() || result.is_infinite() {
        return None;
    }

    Some(result)
}

/// Recursive descent parser for arithmetic expressions
struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(expr: &str) -> Self {
        Self {
            chars: expr.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn parse_expression(&mut self) -> Option<f64> {
        let mut result = self.parse_term()?;

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('+') => {
                    self.advance();
                    let right = self.parse_term()?;
                    result += right;
                }
                Some('-') => {
                    self.advance();
                    let right = self.parse_term()?;
                    result -= right;
                }
                _ => break,
            }
        }

        Some(result)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut result = self.parse_power()?;

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('*') => {
                    // Check for ** (not supported, should be handled by ^)
                    self.advance();
                    if self.peek() == Some('*') {
                        return None; // ** not allowed
                    }
                    let right = self.parse_power()?;
                    result *= right;
                }
                Some('/') => {
                    self.advance();
                    let right = self.parse_power()?;
                    if right == 0.0 {
                        return None;
                    }
                    result /= right;
                }
                Some('%') => {
                    self.advance();
                    let right = self.parse_power()?;
                    if right == 0.0 {
                        return None;
                    }
                    result %= right;
                }
                _ => break,
            }
        }

        Some(result)
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;

        self.skip_whitespace();
        if self.peek() == Some('^') {
            self.advance();
            let exp = self.parse_power()?; // Right-associative
            if base < 0.0 && exp != exp.floor() {
                return None;
            }
            let result = base.powf(exp);
            if result.is_nan() || result.is_infinite() {
                return None;
            }
            Some(result)
        } else {
            Some(base)
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_whitespace();
        if self.peek() == Some('-') {
            self.advance();
            let val = self.parse_unary()?;
            Some(-val)
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_whitespace();

        match self.peek() {
            Some('(') => {
                self.advance();
                let result = self.parse_expression()?;
                self.skip_whitespace();
                if self.peek() != Some(')') {
                    return None;
                }
                self.advance();
                Some(result)
            }
            Some(c) if c.is_ascii_digit() => {
                let mut num_str = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        num_str.push(c);
                        self.advance();
                    } else {
                        break;
                    }
                }
                num_str.parse::<f64>().ok()
            }
            _ => None,
        }
    }
}

/// Check if a value is an integer (matches JS Number.isInteger)
pub fn is_integer(value: f64) -> bool {
    value.is_finite() && value == value.floor()
}

/// Check if a string is a simple number (or negative number)
pub fn is_simple_number_or_negative(expr: &str) -> bool {
    let trimmed = expr.trim();
    if let Some(stripped) = trimmed.strip_prefix('-') {
        !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_digit())
    } else {
        !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit())
    }
}

/// Validate a complete equation expression
pub fn is_valid_equation(expression: &str) -> bool {
    if !check_brackets(expression) {
        return false;
    }

    // Find the main operator (= or >=) at depth 0
    let chars: Vec<char> = expression.chars().collect();
    let mut main_op: Option<String> = None;
    let mut main_op_end_index: usize = 0; // index AFTER the main operator
    let mut depth: i32 = 0;

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if is_open_bracket(ch) {
            depth += 1;
        } else if is_close_bracket(ch) {
            depth -= 1;
        } else if depth == 0 && is_main_operator(ch) {
            match main_op {
                None => {
                    main_op = Some(ch.to_string());
                    main_op_end_index = i + 1;
                }
                Some(ref prev) => {
                    if prev == ">" && ch == '=' {
                        main_op = Some(">=".to_string());
                        main_op_end_index = i + 1;
                    } else if prev == "=" && ch == '=' {
                        return false; // Double = not allowed
                    } else if prev.as_str() != &expression[i..i + 1] {
                        return false; // Different main operators
                    } else {
                        // Same operator repeated
                        if ch == '=' {
                            return false;
                        }
                    }
                }
            }
        }
        i += 1;
    }

    let main_op = match main_op {
        Some(op) => op,
        None => return false,
    };

    if main_op_end_index == 0 || main_op_end_index >= chars.len() {
        return false;
    }

    let left_end = if main_op == ">=" {
        main_op_end_index - 2
    } else {
        main_op_end_index - 1
    };

    let left_side = &expression[..left_end];
    let right_side = &expression[main_op_end_index..];

    if left_side.is_empty() || right_side.is_empty() {
        return false;
    }

    // Check for negative number on RHS
    let has_minus_on_rhs_start = right_side.starts_with('-');
    if has_minus_on_rhs_start && right_side.len() == 1 {
        return false;
    }

    let left_value = evaluate_expression(left_side);
    let right_value = evaluate_expression(right_side);

    match (left_value, right_value) {
        (Some(lv), Some(rv)) => {
            // Both must be integers
            if !is_integer(lv) || !is_integer(rv) {
                return false;
            }

            match main_op.as_str() {
                "=" => {
                    // RHS must be a simple number
                    if !is_simple_number_or_negative(right_side) {
                        return false;
                    }
                    (lv as i64) == (rv as i64)
                }
                ">" => (lv as i64) > (rv as i64),
                ">=" => (lv as i64) >= (rv as i64),
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_brackets() {
        assert!(check_brackets("1+2"));
        assert!(check_brackets("(1+2)"));
        assert!(check_brackets("[7/2]"));
        assert!(check_brackets("[(1+2)/3]"));
        assert!(!check_brackets("(1+2"));
        assert!(!check_brackets("1+2)"));
        assert!(!check_brackets("[1+2)"));
    }

    #[test]
    fn test_evaluate_simple() {
        assert_eq!(evaluate_expression("1+2"), Some(3.0));
        assert_eq!(evaluate_expression("3*4"), Some(12.0));
        assert_eq!(evaluate_expression("10/2"), Some(5.0));
        assert_eq!(evaluate_expression("2^3"), Some(8.0));
        assert_eq!(evaluate_expression("5%3"), Some(2.0));
    }

    #[test]
    fn test_evaluate_factorial() {
        assert_eq!(evaluate_expression("5!"), Some(120.0));
        assert_eq!(evaluate_expression("0!"), Some(1.0));
        assert_eq!(evaluate_expression("12!"), Some(479001600.0));
        assert_eq!(evaluate_expression("13!"), None); // Too large
    }

    #[test]
    fn test_evaluate_permutation() {
        assert_eq!(evaluate_expression("5A3"), Some(60.0)); // 5*4*3
        assert_eq!(evaluate_expression("10A2"), Some(90.0)); // 10*9
    }

    #[test]
    fn test_evaluate_floor() {
        assert_eq!(evaluate_expression("[7/2]"), Some(3.0));
        assert_eq!(evaluate_expression("[5]"), Some(5.0));
    }

    #[test]
    fn test_is_valid_equation() {
        assert!(is_valid_equation("1+2=3"));
        assert!(is_valid_equation("2*3=6"));
        assert!(is_valid_equation("3>2"));
        assert!(!is_valid_equation("1+2")); // No main operator
        assert!(!is_valid_equation("=3")); // No LHS
    }

    #[test]
    fn test_leading_zero() {
        assert_eq!(evaluate_expression("01"), None);
        assert_eq!(evaluate_expression("0"), Some(0.0));
        assert_eq!(evaluate_expression("10"), Some(10.0));
    }

    #[test]
    fn test_complex_expression() {
        assert!(is_valid_equation("1+2*3=7"));
        assert!(is_valid_equation("2^3=8"));
        assert!(is_valid_equation("5!-1=119"));
        assert!(is_valid_equation("[7/2]*2=6"));
    }

    #[test]
    fn test_rhs_must_be_simple() {
        // RHS of = must be a simple number
        assert!(!is_valid_equation("6=2*3")); // RHS is not a simple number
        assert!(is_valid_equation("2*3=6")); // RHS is simple
        assert!(is_valid_equation("5-8=-3")); // RHS is a negative simple number
    }

    #[test]
    fn test_ge_operator() {
        assert!(is_valid_equation("5>=3"));
        assert!(is_valid_equation("3>=3"));
        assert!(!is_valid_equation("2>=3"));
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

    #[test]
    fn test_is_simple_number() {
        assert!(is_simple_number_or_negative("5"));
        assert!(is_simple_number_or_negative("-3"));
        assert!(is_simple_number_or_negative("100"));
        assert!(!is_simple_number_or_negative("2*3"));
        assert!(!is_simple_number_or_negative("(5)"));
    }

    #[test]
    fn test_division_results() {
        // 7/2 = 3.5 (not integer)
        assert_eq!(evaluate_expression("7/2"), Some(3.5));
        // [7/2] = 3 (integer via floor)
        assert_eq!(evaluate_expression("[7/2]"), Some(3.0));
    }

    #[test]
    fn test_unary_minus() {
        assert_eq!(evaluate_expression("-5"), Some(-5.0));
        assert_eq!(evaluate_expression("3--2"), Some(5.0));
    }
}
