//! Expression evaluator for Sumzle equations
//!
//! This evaluator closely follows the behavior of the reference JavaScript
//! implementation, using f64 internally for arithmetic and checking for
//! integer results at the equation validation level.

use crate::types::*;
use std::borrow::Cow;

const FACTORIAL_TABLE: [u64; (MAX_FACTORIAL as usize) + 1] = [
    1, 1, 2, 6, 24, 120, 720, 5040, 40320, 362880, 3628800, 39916800, 479001600,
];

const PERMUTATION_TABLE: [[u64; (MAX_PERMUTATION as usize) + 1]; (MAX_PERMUTATION as usize) + 1] = [
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 3, 6, 6, 0, 0, 0, 0, 0, 0, 0],
    [1, 4, 12, 24, 24, 0, 0, 0, 0, 0, 0],
    [1, 5, 20, 60, 120, 120, 0, 0, 0, 0, 0],
    [1, 6, 30, 120, 360, 720, 720, 0, 0, 0, 0],
    [1, 7, 42, 210, 840, 2520, 5040, 5040, 0, 0, 0],
    [1, 8, 56, 336, 1680, 6720, 20160, 40320, 40320, 0, 0],
    [1, 9, 72, 504, 3024, 15120, 60480, 181440, 362880, 362880, 0],
    [
        1, 10, 90, 720, 5040, 30240, 151200, 604800, 1814400, 3628800, 3628800,
    ],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainOperator {
    Equal,
    Greater,
    GreaterEqual,
}

#[inline]
const fn is_open_bracket_b(ch: u8) -> bool {
    matches!(ch, b'(' | b'[')
}

#[inline]
const fn is_close_bracket_b(ch: u8) -> bool {
    matches!(ch, b')' | b']')
}

#[inline]
const fn is_main_operator_b(ch: u8) -> bool {
    matches!(ch, b'=' | b'>')
}

/// Check if brackets are properly matched
pub fn check_brackets(expr: &str) -> bool {
    check_brackets_bytes(expr.as_bytes())
}

#[inline]
fn check_brackets_bytes(expr: &[u8]) -> bool {
    let mut stack: Vec<u8> = Vec::with_capacity(expr.len().min(8));
    for &ch in expr {
        if is_open_bracket_b(ch) {
            stack.push(ch);
        } else if is_close_bracket_b(ch) {
            let Some(last_open) = stack.pop() else {
                return false;
            };
            if !matches!((last_open, ch), (b'(', b')') | (b'[', b']')) {
                return false;
            }
        }
    }
    stack.is_empty()
}

/// Evaluate a mathematical expression, returning None if invalid.
/// Returns f64 to support fractional intermediate results (e.g., 7/2 = 3.5).
pub fn evaluate_expression(expr: &str) -> Option<f64> {
    evaluate_expression_bytes(expr.as_bytes())
}

#[inline]
pub(crate) fn evaluate_expression_bytes(expr: &[u8]) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut has_floor = false;
    let mut has_factorial = false;
    let mut has_permutation = false;
    for &b in expr {
        match b {
            b'[' | b']' => has_floor = true,
            b'!' => has_factorial = true,
            b'A' => has_permutation = true,
            _ => {}
        }
    }

    if !has_floor && !has_factorial && !has_permutation {
        return evaluate_arithmetic_bytes(expr);
    }

    let mut current: Cow<'_, [u8]> = Cow::Borrowed(expr);

    if has_floor {
        current = Cow::Owned(resolve_floor_brackets_bytes(current.as_ref())?);
    }

    if has_factorial {
        current = Cow::Owned(handle_factorials_bytes(current.as_ref())?);
    }

    if has_permutation {
        current = Cow::Owned(handle_permutations_bytes(current.as_ref())?);
    }

    evaluate_arithmetic_bytes(current.as_ref())
}

/// Fast evaluator for solver-generated expressions. Solver expressions never
/// contain whitespace, so the common arithmetic-only path can avoid the more
/// general whitespace-aware parser while preserving public evaluator behavior.
#[inline]
pub(crate) fn evaluate_expression_solver_bytes(expr: &[u8]) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut needs_float = false;
    for &b in expr {
        match b {
            b'0'..=b'9' | b'+' | b'-' | b'*' | b'%' | b'(' | b')' => {}
            b'[' | b']' | b'!' | b'A' => return evaluate_expression_bytes(expr),
            _ => needs_float = true,
        }
    }

    if needs_float {
        return evaluate_arithmetic_no_ws_bytes(expr);
    }

    // Most solver-generated expressions use integer-only arithmetic. Avoid
    // floating-point parsing for that common path, but fall back on overflow
    // so this optimization cannot reject an expression the reference f64
    // evaluator accepts at very large puzzle lengths.
    evaluate_arithmetic_i64(expr)
        .map(|value| value as f64)
        .or_else(|| evaluate_arithmetic_no_ws_bytes(expr))
}

fn evaluate_arithmetic_i64(expr: &[u8]) -> Option<i64> {
    let mut parser = I64Parser {
        bytes: expr,
        pos: 0,
    };
    let result = parser.parse_expression()?;
    (parser.pos == expr.len()).then_some(result)
}

struct I64Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

// Every integer in this inclusive range has an exact f64 representation.
// Falling back once an intermediate leaves it preserves the numeric values
// relevant to solver comparisons (not merely overflow safety). Signed zero may
// normalize to +0 on this private path, which is immaterial because the solver
// immediately validates and converts the result to i64.
const MAX_EXACT_F64_INTEGER: u64 = 1u64 << 53;

#[inline]
fn exact_f64_integer(value: i64) -> Option<i64> {
    (value.unsigned_abs() <= MAX_EXACT_F64_INTEGER).then_some(value)
}

impl I64Parser<'_> {
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_expression(&mut self) -> Option<i64> {
        let mut result = self.parse_term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.advance();
                    result = exact_f64_integer(result.checked_add(self.parse_term()?)?)?;
                }
                Some(b'-') => {
                    self.advance();
                    result = exact_f64_integer(result.checked_sub(self.parse_term()?)?)?;
                }
                _ => return Some(result),
            }
        }
    }

    fn parse_term(&mut self) -> Option<i64> {
        let mut result = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.advance();
                    if self.peek() == Some(b'*') {
                        return None;
                    }
                    result = exact_f64_integer(result.checked_mul(self.parse_unary()?)?)?;
                }
                Some(b'%') => {
                    self.advance();
                    result = exact_f64_integer(result.checked_rem(self.parse_unary()?)?)?;
                }
                _ => return Some(result),
            }
        }
    }

    fn parse_unary(&mut self) -> Option<i64> {
        if self.peek() == Some(b'-') {
            self.advance();
            exact_f64_integer(self.parse_unary()?.checked_neg()?)
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<i64> {
        match self.peek() {
            Some(b'(') => {
                self.advance();
                let result = self.parse_expression()?;
                if self.peek() != Some(b')') {
                    return None;
                }
                self.advance();
                Some(result)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                let mut value = 0i64;
                while let Some(c) = self.peek() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    value =
                        exact_f64_integer(value.checked_mul(10)?.checked_add((c - b'0') as i64)?)?;
                    self.advance();
                }
                if self.pos - start > 1 && self.bytes[start] == b'0' {
                    return None;
                }
                Some(value)
            }
            _ => None,
        }
    }
}

fn resolve_floor_brackets_bytes(expr: &[u8]) -> Option<Vec<u8>> {
    let mut processed = expr.to_vec();
    let mut bracket_iterations = 0;
    const MAX_BRACKET_ITERATIONS: usize = 10;

    while bracket_iterations < MAX_BRACKET_ITERATIONS {
        if let Some(start) = processed.iter().rposition(|&b| b == b'[') {
            let end_rel = processed[start + 1..].iter().position(|&b| b == b']')?;
            let inner_start = start + 1;
            let inner_end = start + 1 + end_rel;
            let inner_expr = &processed[inner_start..inner_end];

            if inner_expr.is_empty() {
                return None;
            }

            let inner_val = evaluate_inner_expression_bytes(inner_expr)?;
            if !inner_val.is_finite() {
                return None;
            }

            let floored = inner_val.floor() as i64;
            let mut next = Vec::with_capacity(processed.len() + 16);
            next.extend_from_slice(&processed[..start]);
            append_i64_decimal(&mut next, floored);
            next.extend_from_slice(&processed[inner_end + 1..]);
            processed = next;
        } else {
            break;
        }
        bracket_iterations += 1;
    }

    if bracket_iterations >= MAX_BRACKET_ITERATIONS && processed.contains(&b'[') {
        return None;
    }

    Some(processed)
}

/// Evaluate an expression inside floor brackets (no nested brackets expected)
#[inline]
fn evaluate_inner_expression_bytes(expr: &[u8]) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut has_factorial = false;
    let mut has_permutation = false;
    for &b in expr {
        match b {
            b'!' => has_factorial = true,
            b'A' => has_permutation = true,
            _ => {}
        }
    }

    if !has_factorial && !has_permutation {
        return evaluate_arithmetic_bytes(expr);
    }

    let mut current: Cow<'_, [u8]> = Cow::Borrowed(expr);

    if has_factorial {
        current = Cow::Owned(handle_factorials_bytes(current.as_ref())?);
    }

    if has_permutation {
        current = Cow::Owned(handle_permutations_bytes(current.as_ref())?);
    }

    evaluate_arithmetic_bytes(current.as_ref())
}

/// Handle factorial expressions in the byte string
fn handle_factorials_bytes(expr: &[u8]) -> Option<Vec<u8>> {
    let mut result: Vec<u8> = Vec::with_capacity(expr.len() + 8);
    let mut i = 0;

    while i < expr.len() {
        let ch = expr[i];
        if ch == b'!' {
            if result.is_empty() {
                return None;
            }

            let mut j = result.len();
            while j > 0 && result[j - 1].is_ascii_digit() {
                j -= 1;
            }

            if j == result.len() {
                return None;
            }

            let n = parse_u64_digits(&result[j..])?;
            if n > MAX_FACTORIAL {
                return None;
            }

            let factorial = compute_factorial(n)?;
            result.truncate(j);
            append_u64_decimal(&mut result, factorial);
        } else {
            result.push(ch);
        }
        i += 1;
    }

    Some(result)
}

/// Compute factorial
#[inline]
fn compute_factorial(n: u64) -> Option<u64> {
    FACTORIAL_TABLE.get(n as usize).copied()
}

/// Handle permutation expressions (nAr = n!/(n-r)!)
fn handle_permutations_bytes(expr: &[u8]) -> Option<Vec<u8>> {
    let mut result: Vec<u8> = Vec::with_capacity(expr.len() + 8);
    let mut i = 0;

    while i < expr.len() {
        let ch = expr[i];
        if ch == b'A' {
            if result.is_empty() {
                return None;
            }

            let mut j = result.len();
            while j > 0 && result[j - 1].is_ascii_digit() {
                j -= 1;
            }

            if j == result.len() {
                return None;
            }

            let m = parse_u64_digits(&result[j..])?;

            let mut k = i + 1;
            while k < expr.len() && expr[k].is_ascii_digit() {
                k += 1;
            }

            if k == i + 1 {
                return None;
            }

            let n = parse_u64_digits(&expr[i + 1..k])?;
            if m > MAX_PERMUTATION || n > MAX_PERMUTATION || n > m {
                return None;
            }

            let perm = compute_permutation(m, n)?;
            result.truncate(j);
            append_u64_decimal(&mut result, perm);
            i = k;
            continue;
        } else {
            result.push(ch);
        }
        i += 1;
    }

    Some(result)
}

/// Compute permutation P(m,n) = m!/(m-n)!
#[inline]
fn compute_permutation(m: u64, n: u64) -> Option<u64> {
    PERMUTATION_TABLE
        .get(m as usize)
        .and_then(|row| row.get(n as usize))
        .copied()
}

#[inline]
fn parse_u64_digits(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((b - b'0') as u64)?;
    }
    Some(value)
}

#[inline]
fn append_u64_decimal(buf: &mut Vec<u8>, mut n: u64) {
    if n == 0 {
        buf.push(b'0');
        return;
    }

    let mut tmp = [0u8; 20];
    let mut idx = tmp.len();
    while n > 0 {
        idx -= 1;
        tmp[idx] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf.extend_from_slice(&tmp[idx..]);
}

#[inline]
fn append_i64_decimal(buf: &mut Vec<u8>, n: i64) {
    if n < 0 {
        buf.push(b'-');
        // `unsigned_abs` instead of `-n`: negating i64::MIN overflows and panics
        // in debug builds, and this path is reachable from public evaluator input.
        append_u64_decimal(buf, n.unsigned_abs());
    } else {
        append_u64_decimal(buf, n as u64);
    }
}

/// Evaluate a simple arithmetic expression using a recursive descent parser.
/// Supports: +, -, *, /, %, ^, parentheses
fn evaluate_arithmetic_bytes(expr: &[u8]) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut parser = Parser::new(expr);
    let result = parser.parse_expression()?;
    parser.skip_whitespace();

    if parser.pos < parser.bytes.len() || result.is_nan() || result.is_infinite() {
        return None;
    }

    Some(result)
}

/// Recursive descent parser for arithmetic expressions
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    #[inline]
    fn new(expr: &'a [u8]) -> Self {
        Self {
            bytes: expr,
            pos: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn advance(&mut self) -> Option<u8> {
        let ch = self.bytes.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    #[inline]
    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn parse_expression(&mut self) -> Option<f64> {
        let mut result = self.parse_term()?;

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.advance();
                    result += self.parse_term()?;
                }
                Some(b'-') => {
                    self.advance();
                    result -= self.parse_term()?;
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
                Some(b'*') => {
                    self.advance();
                    if self.peek() == Some(b'*') {
                        return None;
                    }
                    result *= self.parse_power()?;
                }
                Some(b'/') => {
                    self.advance();
                    let right = self.parse_power()?;
                    if right == 0.0 {
                        return None;
                    }
                    result /= right;
                }
                Some(b'%') => {
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
        if self.peek() == Some(b'^') {
            self.advance();
            let exp = self.parse_power()?;
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
        if self.peek() == Some(b'-') {
            self.advance();
            Some(-self.parse_unary()?)
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_whitespace();

        match self.peek() {
            Some(b'(') => {
                self.advance();
                let result = self.parse_expression()?;
                self.skip_whitespace();
                if self.peek() != Some(b')') {
                    return None;
                }
                self.advance();
                Some(result)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                let mut value = 0.0f64;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        value = value * 10.0 + (c - b'0') as f64;
                        self.advance();
                    } else {
                        break;
                    }
                }
                if self.pos - start > 1 && self.bytes[start] == b'0' {
                    return None;
                }
                Some(value)
            }
            _ => None,
        }
    }
}

#[inline]
fn evaluate_arithmetic_no_ws_bytes(expr: &[u8]) -> Option<f64> {
    if expr.is_empty() {
        return None;
    }

    let mut parser = NoWsParser::new(expr);
    let result = parser.parse_expression()?;
    if parser.pos < parser.bytes.len() || result.is_nan() || result.is_infinite() {
        return None;
    }
    Some(result)
}

struct NoWsParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> NoWsParser<'a> {
    #[inline]
    fn new(expr: &'a [u8]) -> Self {
        Self {
            bytes: expr,
            pos: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_expression(&mut self) -> Option<f64> {
        let mut result = self.parse_term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.advance();
                    result += self.parse_term()?;
                }
                Some(b'-') => {
                    self.advance();
                    result -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Some(result)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut result = self.parse_power()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.advance();
                    if self.peek() == Some(b'*') {
                        return None;
                    }
                    result *= self.parse_power()?;
                }
                Some(b'/') => {
                    self.advance();
                    let right = self.parse_power()?;
                    if right == 0.0 {
                        return None;
                    }
                    result /= right;
                }
                Some(b'%') => {
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
        if self.peek() == Some(b'^') {
            self.advance();
            let exp = self.parse_power()?;
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
        if self.peek() == Some(b'-') {
            self.advance();
            Some(-self.parse_unary()?)
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(b'(') => {
                self.advance();
                let result = self.parse_expression()?;
                if self.peek() != Some(b')') {
                    return None;
                }
                self.advance();
                Some(result)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                let mut value = 0.0f64;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        value = value * 10.0 + (c - b'0') as f64;
                        self.advance();
                    } else {
                        break;
                    }
                }
                if self.pos - start > 1 && self.bytes[start] == b'0' {
                    return None;
                }
                Some(value)
            }
            _ => None,
        }
    }
}

/// Check if a value is an integer (matches JS Number.isInteger)
pub fn is_integer(value: f64) -> bool {
    value.is_finite() && value == value.floor()
}

#[inline]
fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

#[inline]
fn is_simple_number_or_negative_bytes(expr: &[u8]) -> bool {
    let trimmed = trim_ascii_whitespace(expr);
    if let Some(stripped) = trimmed.strip_prefix(b"-") {
        !stripped.is_empty() && stripped.iter().all(|b| b.is_ascii_digit())
    } else {
        !trimmed.is_empty() && trimmed.iter().all(|b| b.is_ascii_digit())
    }
}

#[inline]
fn parse_simple_number_or_negative_value_bytes(expr: &[u8]) -> Option<f64> {
    let trimmed = trim_ascii_whitespace(expr);
    let (negative, digits) = if let Some(stripped) = trimmed.strip_prefix(b"-") {
        (true, stripped)
    } else {
        (false, trimmed)
    };

    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }

    let mut value = 0.0f64;
    for &b in digits {
        value = value * 10.0 + (b - b'0') as f64;
    }

    Some(if negative { -value } else { value })
}

/// Check if a string is a simple number (or negative number)
pub fn is_simple_number_or_negative(expr: &str) -> bool {
    is_simple_number_or_negative_bytes(expr.as_bytes())
}

#[inline]
fn is_valid_equation_bytes_impl(expression: &[u8], check_brackets: bool) -> bool {
    if check_brackets && !check_brackets_bytes(expression) {
        return false;
    }

    let mut main_op: Option<MainOperator> = None;
    let mut main_op_end_index: usize = 0;
    let mut depth: i32 = 0;

    let mut i = 0;
    while i < expression.len() {
        let ch = expression[i];
        if is_open_bracket_b(ch) {
            depth += 1;
        } else if is_close_bracket_b(ch) {
            depth -= 1;
        } else if depth == 0 && is_main_operator_b(ch) {
            match main_op {
                None => {
                    main_op = Some(if ch == b'=' {
                        MainOperator::Equal
                    } else {
                        MainOperator::Greater
                    });
                    main_op_end_index = i + 1;
                }
                Some(MainOperator::Greater) if ch == b'=' => {
                    main_op = Some(MainOperator::GreaterEqual);
                    main_op_end_index = i + 1;
                }
                Some(MainOperator::Equal) if ch == b'=' => return false,
                Some(MainOperator::Equal) => return false,
                Some(MainOperator::GreaterEqual) => return false,
                Some(MainOperator::Greater) => {
                    // Preserve prior behavior for repeated '>' by letting RHS parsing reject it.
                }
            }
        }
        i += 1;
    }

    let Some(main_op) = main_op else {
        return false;
    };

    if main_op_end_index == 0 || main_op_end_index >= expression.len() {
        return false;
    }

    let left_end = if main_op == MainOperator::GreaterEqual {
        main_op_end_index - 2
    } else {
        main_op_end_index - 1
    };

    let left_side = &expression[..left_end];
    let right_side = &expression[main_op_end_index..];

    if left_side.is_empty() || right_side.is_empty() || trim_ascii_whitespace(right_side) == b"-" {
        return false;
    }

    match main_op {
        MainOperator::Equal => {
            let Some(rv) = parse_simple_number_or_negative_value_bytes(right_side) else {
                return false;
            };
            let Some(lv) = evaluate_expression_bytes(left_side) else {
                return false;
            };
            // `is_integer(rv)` rejects a non-finite RHS: a very long numeric
            // literal parses to ±∞, whose `as i64` cast saturates to
            // i64::MAX/MIN and could spuriously equal a saturated LHS.
            is_integer(lv) && is_integer(rv) && (lv as i64) == (rv as i64)
        }
        MainOperator::Greater | MainOperator::GreaterEqual => {
            let left_value = evaluate_expression_bytes(left_side);
            let right_value = evaluate_expression_bytes(right_side);
            match (left_value, right_value) {
                (Some(lv), Some(rv)) if is_integer(lv) && is_integer(rv) => match main_op {
                    MainOperator::Greater => (lv as i64) > (rv as i64),
                    MainOperator::GreaterEqual => (lv as i64) >= (rv as i64),
                    MainOperator::Equal => unreachable!(),
                },
                _ => false,
            }
        }
    }
}

#[inline]
pub(crate) fn is_valid_equation_bytes(expression: &[u8]) -> bool {
    is_valid_equation_bytes_impl(expression, true)
}

/// Validate a complete equation expression
pub fn is_valid_equation(expression: &str) -> bool {
    is_valid_equation_bytes(expression.as_bytes())
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

    #[test]
    fn solver_integer_fast_path_falls_back_on_i64_overflow() {
        let expr = b"1000000*1000000*1000000*1000000";
        assert_eq!(
            evaluate_expression_solver_bytes(expr),
            evaluate_expression_bytes(expr)
        );
    }

    #[test]
    fn solver_integer_fast_path_preserves_f64_rounding() {
        let expr = b"1000000*1000000*10000+1-1000000*1000000*10000";
        assert_eq!(evaluate_expression_bytes(expr), Some(0.0));
        assert_eq!(
            evaluate_expression_solver_bytes(expr),
            evaluate_expression_bytes(expr)
        );
    }
}
