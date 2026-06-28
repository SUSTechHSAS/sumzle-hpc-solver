//! Incremental expression evaluator (A1+A2).
//!
//! Maintains a Shunting-yard RPN state incrementally as the DFS places
//! characters. At the main operator, the LHS value is read via `peek_value`
//! in O(rpn_len) instead of re-parsing the entire LHS.
//!
//! # Backtracking
//!
//! Push may reduce (pop ops to RPN output), but undo is O(1): a checkpoint
//! records pre-push stack lengths. Popped ops sit below the new op's position
//! and are restored by truncation.
//!
//! # Floor brackets
//!
//! Floor brackets `[...]` are NOT handled incrementally (the reference
//! evaluator uses string replacement with quirky digit-concatenation
//! behavior). When `[` is pushed, the evaluator sets `invalid=true`, causing
//! `peek_value` to return `None` — the caller falls back to the reference
//! evaluator.

use crate::evaluator::is_integer;

pub const MAX_EVAL_LEN: usize = 40;

#[derive(Clone, Copy)]
enum Token {
    Val(f64),
    Bin(u8),
    Neg,
    Fact,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Op {
    Add = 0,
    Sub = 1,
    Mul = 2,
    Div = 3,
    Mod = 4,
    Pow = 5,
    PermA = 6,
    Neg = 7,
    LParen = 8,
    LBracket = 9,
}

impl Op {
    #[inline]
    const fn from_byte(ch: u8) -> Option<Op> {
        match ch {
            b'+' => Some(Op::Add),
            b'-' => Some(Op::Sub),
            b'*' => Some(Op::Mul),
            b'/' => Some(Op::Div),
            b'%' => Some(Op::Mod),
            b'^' => Some(Op::Pow),
            b'A' => Some(Op::PermA),
            b'(' => Some(Op::LParen),
            b'[' => Some(Op::LBracket),
            _ => None,
        }
    }

    #[inline]
    const fn precedence(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div | Op::Mod => 2,
            Op::Pow => 3,
            // PermA higher than Neg (reference pre-processes nAm before parse)
            Op::PermA => 6,
            Op::Neg => 5,
            Op::LParen | Op::LBracket => 0,
        }
    }

    #[inline]
    const fn is_right_assoc(self) -> bool {
        matches!(self, Op::Pow | Op::Neg)
    }

    #[inline]
    const fn is_marker(self) -> bool {
        matches!(self, Op::LParen | Op::LBracket)
    }

    #[inline]
    const fn is_binary(self) -> bool {
        matches!(
            self,
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow | Op::PermA
        )
    }

    #[inline]
    const fn to_bin_token(self) -> Token {
        match self {
            Op::Add => Token::Bin(0),
            Op::Sub => Token::Bin(1),
            Op::Mul => Token::Bin(2),
            Op::Div => Token::Bin(3),
            Op::Mod => Token::Bin(4),
            Op::Pow => Token::Bin(5),
            Op::PermA => Token::Bin(6),
            _ => Token::Bin(0),
        }
    }
}

#[inline]
fn apply_binary_tok(tok: u8, a: f64, b: f64) -> Option<f64> {
    let r = match tok {
        0 => a + b,
        1 => a - b,
        2 => a * b,
        3 => {
            if b == 0.0 {
                return None;
            }
            a / b
        }
        4 => {
            if b == 0.0 {
                return None;
            }
            a % b
        }
        5 => {
            if a < 0.0 && b != b.floor() {
                return None;
            }
            a.powf(b)
        }
        6 => return apply_permutation(a, b),
        _ => return None,
    };
    if r.is_nan() || r.is_infinite() {
        None
    } else {
        Some(r)
    }
}

#[inline]
fn apply_factorial(n: f64) -> Option<f64> {
    if !is_integer(n) {
        return None;
    }
    let n = n as i64;
    if n < 0 || n as u64 > crate::types::MAX_FACTORIAL {
        return None;
    }
    let mut r: u64 = 1;
    for i in 2..=n as u64 {
        r *= i;
    }
    Some(r as f64)
}

#[inline]
fn apply_permutation(m: f64, n: f64) -> Option<f64> {
    if !is_integer(m) || !is_integer(n) {
        return None;
    }
    let m = m as i64;
    let n = n as i64;
    if m < 0
        || n < 0
        || m as u64 > crate::types::MAX_PERMUTATION
        || n as u64 > crate::types::MAX_PERMUTATION
        || n > m
    {
        return None;
    }
    let mut r: u64 = 1;
    for i in 0..n {
        r *= (m - i) as u64;
    }
    Some(r as f64)
}

#[derive(Clone, Copy)]
struct Checkpoint {
    rpn_len: u8,
    ops_len: u8,
    has_current: bool,
    current: i64,
    invalid: bool,
}

impl Checkpoint {
    const fn empty() -> Self {
        Self {
            rpn_len: 0,
            ops_len: 0,
            has_current: false,
            current: 0,
            invalid: false,
        }
    }
}

pub struct IncEval {
    rpn: [Token; MAX_EVAL_LEN],
    rpn_len: usize,
    ops: [Op; MAX_EVAL_LEN],
    ops_len: usize,
    current: Option<i64>,
    checkpoints: [Checkpoint; MAX_EVAL_LEN],
    checkpoint_len: usize,
    invalid: bool,
}

impl IncEval {
    pub fn new() -> Self {
        Self {
            rpn: [Token::Val(0.0); MAX_EVAL_LEN],
            rpn_len: 0,
            ops: [Op::Add; MAX_EVAL_LEN],
            ops_len: 0,
            current: None,
            checkpoints: [Checkpoint::empty(); MAX_EVAL_LEN],
            checkpoint_len: 0,
            invalid: false,
        }
    }

    pub fn reset(&mut self) {
        self.rpn_len = 0;
        self.ops_len = 0;
        self.current = None;
        self.checkpoint_len = 0;
        self.invalid = false;
    }

    #[inline]
    fn push_checkpoint(&mut self) {
        if self.checkpoint_len >= MAX_EVAL_LEN {
            // Checkpoint stack overflow: mark invalid so peek_value returns
            // None and the caller falls back to the reference evaluator.
            // A silent return would leave a missing checkpoint, causing the
            // next undo() to restore a stale state and corrupt backtracking.
            self.invalid = true;
            return;
        }
        let idx = self.checkpoint_len;
        self.checkpoint_len += 1;
        let cp = &mut self.checkpoints[idx];
        cp.rpn_len = self.rpn_len as u8;
        cp.ops_len = self.ops_len as u8;
        cp.has_current = self.current.is_some();
        cp.current = self.current.unwrap_or(0);
        cp.invalid = self.invalid;
    }

    #[inline]
    fn emit_val(&mut self, v: f64) {
        if self.rpn_len < MAX_EVAL_LEN {
            self.rpn[self.rpn_len] = Token::Val(v);
            self.rpn_len += 1;
        } else {
            self.invalid = true;
        }
    }

    #[inline]
    fn emit_tok(&mut self, t: Token) {
        if self.rpn_len < MAX_EVAL_LEN {
            self.rpn[self.rpn_len] = t;
            self.rpn_len += 1;
        } else {
            self.invalid = true;
        }
    }

    #[inline]
    fn finalize_current(&mut self) {
        if let Some(v) = self.current.take() {
            self.emit_val(v as f64);
        }
    }

    #[inline]
    fn pop_op_to_rpn(&mut self) {
        if self.ops_len > 0 {
            self.ops_len -= 1;
            let op = self.ops[self.ops_len];
            match op {
                Op::Neg => self.emit_tok(Token::Neg),
                _ => self.emit_tok(op.to_bin_token()),
            }
        }
    }

    #[inline]
    fn reduce_while(&mut self, new_op: Op) {
        let new_prec = new_op.precedence();
        let right = new_op.is_right_assoc();
        while self.ops_len > 0 {
            let top = self.ops[self.ops_len - 1];
            if top.is_marker() {
                break;
            }
            let should = if right {
                top.precedence() > new_prec
            } else {
                top.precedence() >= new_prec
            };
            if !should {
                break;
            }
            self.pop_op_to_rpn();
        }
    }

    #[inline]
    pub fn push(&mut self, ch: u8, prev_char: Option<u8>) -> bool {
        // Check invalid BEFORE push_checkpoint so a failed push does not
        // leave an orphaned checkpoint on the stack. If push returns false,
        // the caller will NOT call undo(), so no checkpoint should be pushed.
        if self.invalid {
            return false;
        }

        self.push_checkpoint();

        if ch.is_ascii_digit() {
            let digit = (ch - b'0') as i64;
            let new_val = match self.current {
                Some(v) => v.checked_mul(10).and_then(|v| v.checked_add(digit)),
                None => Some(digit),
            };
            match new_val {
                Some(v) => {
                    self.current = Some(v);
                    return true;
                }
                None => {
                    // Overflow: pop the checkpoint we just pushed so the
                    // stack stays synchronized (caller won't call undo).
                    self.checkpoint_len -= 1;
                    self.invalid = true;
                    return false;
                }
            }
        }

        self.finalize_current();

        match ch {
            b'+' | b'*' | b'/' | b'%' | b'^' | b'A' => {
                let op = Op::from_byte(ch).unwrap();
                self.reduce_while(op);
                if self.ops_len < MAX_EVAL_LEN {
                    self.ops[self.ops_len] = op;
                    self.ops_len += 1;
                } else {
                    self.invalid = true;
                }
            }
            b'-' => {
                // Unary minus if at start or after operator/open-bracket/main-op.
                // After `!` it's BINARY (reference replaces n! with number).
                let is_unary = match prev_char {
                    None => true,
                    Some(pc) => !pc.is_ascii_digit() && pc != b')' && pc != b']' && pc != b'!',
                };
                let op = if is_unary { Op::Neg } else { Op::Sub };
                self.reduce_while(op);
                if self.ops_len < MAX_EVAL_LEN {
                    self.ops[self.ops_len] = op;
                    self.ops_len += 1;
                } else {
                    self.invalid = true;
                }
            }
            b'(' => {
                if self.ops_len < MAX_EVAL_LEN {
                    self.ops[self.ops_len] = Op::LParen;
                    self.ops_len += 1;
                } else {
                    self.invalid = true;
                }
            }
            b'[' | b']' => {
                // Floor brackets: fall back to reference evaluator.
                self.invalid = true;
            }
            b')' => {
                let mut found = false;
                while self.ops_len > 0 {
                    let top = self.ops[self.ops_len - 1];
                    if top == Op::LParen {
                        self.ops_len -= 1;
                        found = true;
                        break;
                    }
                    if top == Op::LBracket {
                        self.invalid = true;
                        return false;
                    }
                    self.pop_op_to_rpn();
                }
                if !found {
                    self.invalid = true;
                    return false;
                }
            }
            b'!' => {
                // Reference: `!` rejected after `)`, accepted after `]` (floor
                // replaced with number first). Since `[` sets invalid, we only
                // see `!` after digits here. Reject after `)`.
                if prev_char == Some(b')') {
                    self.invalid = true;
                    return false;
                }
                self.emit_tok(Token::Fact);
            }
            _ => {
                self.invalid = true;
            }
        }
        true
    }

    #[inline]
    pub fn undo(&mut self) {
        if self.checkpoint_len == 0 {
            return;
        }
        self.checkpoint_len -= 1;
        let cp = self.checkpoints[self.checkpoint_len];
        self.rpn_len = cp.rpn_len as usize;
        self.ops_len = cp.ops_len as usize;
        self.current = if cp.has_current {
            Some(cp.current)
        } else {
            None
        };
        self.invalid = cp.invalid;
    }

    /// Save current state (checkpoint) then reset to clean slate for RHS.
    #[inline]
    pub fn save_and_reset(&mut self) {
        self.push_checkpoint();
        self.rpn_len = 0;
        self.ops_len = 0;
        self.current = None;
        self.invalid = false;
    }

    #[inline]
    pub fn is_invalid(&self) -> bool {
        self.invalid
    }

    pub fn peek_value(&self) -> Option<f64> {
        if self.invalid {
            return None;
        }

        let mut stack: [f64; MAX_EVAL_LEN] = [0.0; MAX_EVAL_LEN];
        let mut stack_len: usize = 0;

        for i in 0..self.rpn_len {
            match self.rpn[i] {
                Token::Val(v) => {
                    if stack_len >= MAX_EVAL_LEN {
                        return None;
                    }
                    stack[stack_len] = v;
                    stack_len += 1;
                }
                Token::Bin(tok) => {
                    if stack_len < 2 {
                        return None;
                    }
                    let b = stack[stack_len - 1];
                    let a = stack[stack_len - 2];
                    match apply_binary_tok(tok, a, b) {
                        Some(r) => {
                            stack[stack_len - 2] = r;
                            stack_len -= 1;
                        }
                        None => return None,
                    }
                }
                Token::Neg => {
                    if stack_len < 1 {
                        return None;
                    }
                    stack[stack_len - 1] = -stack[stack_len - 1];
                }
                Token::Fact => {
                    if stack_len < 1 {
                        return None;
                    }
                    match apply_factorial(stack[stack_len - 1]) {
                        Some(r) => stack[stack_len - 1] = r,
                        None => return None,
                    }
                }
            }
        }

        if let Some(v) = self.current {
            if stack_len >= MAX_EVAL_LEN {
                return None;
            }
            stack[stack_len] = v as f64;
            stack_len += 1;
        }

        // Reduce remaining ops on ops stack.
        let mut ops_len = self.ops_len;
        while ops_len > 0 {
            ops_len -= 1;
            let op = self.ops[ops_len];
            match op {
                Op::Neg => {
                    if stack_len < 1 {
                        return None;
                    }
                    stack[stack_len - 1] = -stack[stack_len - 1];
                }
                Op::LParen | Op::LBracket => return None,
                _ => {
                    if !op.is_binary() {
                        return None;
                    }
                    if stack_len < 2 {
                        return None;
                    }
                    let b = stack[stack_len - 1];
                    let a = stack[stack_len - 2];
                    let tok = match op {
                        Op::Add => 0u8,
                        Op::Sub => 1u8,
                        Op::Mul => 2u8,
                        Op::Div => 3u8,
                        Op::Mod => 4u8,
                        Op::Pow => 5u8,
                        Op::PermA => 6u8,
                        _ => return None,
                    };
                    match apply_binary_tok(tok, a, b) {
                        Some(r) => {
                            stack[stack_len - 2] = r;
                            stack_len -= 1;
                        }
                        None => return None,
                    }
                }
            }
        }

        if stack_len == 1 {
            let r = stack[0];
            if r.is_nan() || r.is_infinite() {
                None
            } else {
                Some(r)
            }
        } else {
            None
        }
    }
}

impl Default for IncEval {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_inc(expr: &[u8]) -> Option<f64> {
        let mut ev = IncEval::new();
        let mut prev: Option<u8> = None;
        for &ch in expr {
            ev.push(ch, prev);
            prev = Some(ch);
        }
        ev.peek_value()
    }

    fn eval_ref(expr: &[u8]) -> Option<f64> {
        crate::evaluator::evaluate_expression_solver_bytes(expr)
    }

    fn check(expr: &str) {
        let ref_ = eval_ref(expr.as_bytes());
        if expr.as_bytes().contains(&b'[') || expr.as_bytes().contains(&b']') {
            return;
        }
        let inc = eval_inc(expr.as_bytes());
        if let Some(r) = ref_ {
            assert_eq!(inc, Some(r), "mismatch for expr={:?}", expr);
        }
    }

    #[test]
    fn test_basic() {
        check("1+2");
        check("5-3");
        check("3*4");
        check("10/2");
        check("7%3");
    }

    #[test]
    fn test_precedence() {
        check("1+2*3");
        check("2^3+1");
        check("2*3^2");
        check("2^3^2");
    }

    #[test]
    fn test_brackets() {
        check("(1+2)*3");
        check("((1+2))");
        check("(1+2)*(3+4)");
    }

    #[test]
    fn test_unary() {
        check("-5");
        check("3--2");
        check("3*-2");
        check("-(1+2)");
    }

    #[test]
    fn test_factorial() {
        check("5!");
        check("0!");
        check("3!+1");
        check("5!*2");
        check("-5!");
    }

    #[test]
    fn test_permutation() {
        check("5A3");
        check("10A2");
        check("5A3+1");
        check("-7A6");
    }

    #[test]
    fn test_complex() {
        check("(2+3)*7^2");
        check("2*3+4*5-6/2");
        check("5!*2-1");
    }

    #[test]
    fn test_mult_zero() {
        let mut ev = IncEval::new();
        ev.push(b'1', None);
        ev.push(b'*', Some(b'1'));
        ev.push(b'0', Some(b'*'));
        assert_eq!(ev.peek_value(), Some(0.0));
    }

    #[test]
    fn test_dfs_undo() {
        let mut ev = IncEval::new();
        ev.push(b'5', None);
        assert_eq!(ev.peek_value(), Some(5.0));
        ev.undo();
        ev.push(b'1', None);
        ev.push(b'+', Some(b'1'));
        ev.push(b'1', Some(b'+'));
        assert_eq!(ev.peek_value(), Some(2.0));
        ev.undo();
        ev.undo();
        ev.undo();
        ev.push(b'5', None);
        assert_eq!(ev.peek_value(), Some(5.0));
    }

    #[test]
    fn test_save_and_reset_undo() {
        let mut ev = IncEval::new();
        ev.push(b'1', None);
        ev.push(b'+', Some(b'1'));
        ev.push(b'2', Some(b'+'));
        assert_eq!(ev.peek_value(), Some(3.0));
        ev.save_and_reset();
        assert_eq!(ev.peek_value(), None);
        ev.push(b'0', None);
        assert_eq!(ev.peek_value(), Some(0.0));
        ev.undo();
        ev.undo();
        assert_eq!(ev.peek_value(), Some(3.0));
    }

    #[test]
    fn test_fuzz() {
        use std::collections::HashSet;
        let charset = b"0123456789+-*/%^()![]>A";
        let mut s: u64 = 0x1234567890ABCDEF;
        let mut rng = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let mut tested: HashSet<String> = HashSet::new();
        for _ in 0..50000 {
            let len = 3 + (rng() % 6) as usize;
            let mut expr = String::new();
            let mut prev: Option<u8> = None;
            for _ in 0..len {
                let ch = charset[(rng() as usize) % charset.len()];
                if let Some(p) = prev {
                    if p == b'!' && ch.is_ascii_digit() {
                        continue;
                    }
                }
                expr.push(ch as char);
                prev = Some(ch);
            }
            if expr.len() < 3 {
                continue;
            }
            if tested.insert(expr.clone()) {
                check(&expr);
            }
        }
    }
}
