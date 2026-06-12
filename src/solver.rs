//! Brute-force search solver with pruning for Sumzle

use crate::evaluator::is_valid_equation_solver_with_main_bytes;
use crate::types::*;

const CHARSET_LEN: usize = 24;
const NO_CHAR: u8 = 0;

const FLOOR_NO_SLASH: &[u8] = b"0123456789/";
const FLOOR_WITH_SLASH: &[u8] = b"0123456789]";
const AFTER_EQ_START: &[u8] = b"-0123456789";
const AFTER_EQ: &[u8] = b"0123456789";
const FIRST_POSITION: &[u8] = b"123456789([";
const AFTER_DIGIT: &[u8] = b"0123456789+-*/%^A!)][=>";
const AFTER_BINARY_OR_OPEN: &[u8] = b"1234567890([";
const AFTER_CLOSE_OR_FACTORIAL: &[u8] = b"+-*/%^A!)][=>";
const DEFAULT_ORDER: &[u8] = b"1234567890+-*/=([)]%^!A>";
const END_CHARS: &[u8] = b"0123456789)]!";
const LENGTH_ONE_DIGITS: &[u8] = b"0123456789";

#[derive(Debug, Clone)]
struct PreparedKnowledge {
    fixed_chars: Vec<u8>,
    cannot_be_at_masks: Vec<u32>,
    globally_forbidden_mask: u32,
    min_counts: [u8; CHARSET_LEN],
    exact_counts: [u8; CHARSET_LEN],
    exact_mask: u32,
    constrained_indices: Vec<usize>,
}

impl PreparedKnowledge {
    fn new(length: usize, gk: &GlobalKnowledge) -> Self {
        let mut fixed_chars = vec![NO_CHAR; length];
        for (i, fixed) in gk.fixed_chars.iter().enumerate() {
            fixed_chars[i] = fixed.map(|c| c as u8).unwrap_or(NO_CHAR);
        }

        let mut cannot_be_at_masks = vec![0u32; length];
        for (i, set) in gk.cannot_be_at.iter().enumerate() {
            let mut mask = 0u32;
            for &ch in set {
                if let Some(idx) = idx_of_char(ch) {
                    mask |= 1u32 << idx;
                }
            }
            cannot_be_at_masks[i] = mask;
        }

        let mut globally_forbidden_mask = 0u32;
        for &ch in &gk.globally_forbidden {
            if let Some(idx) = idx_of_char(ch) {
                globally_forbidden_mask |= 1u32 << idx;
            }
        }

        let mut min_counts = [0u8; CHARSET_LEN];
        let mut exact_counts = [0u8; CHARSET_LEN];
        let mut exact_mask = 0u32;
        let mut constrained_mask = 0u32;

        for (&ch, &count) in &gk.must_appear_min_count {
            if let Some(idx) = idx_of_char(ch) {
                min_counts[idx] = count as u8;
                constrained_mask |= 1u32 << idx;
            }
        }

        for (&ch, &count) in &gk.must_appear_exact_count {
            if let Some(idx) = idx_of_char(ch) {
                exact_counts[idx] = count as u8;
                exact_mask |= 1u32 << idx;
                constrained_mask |= 1u32 << idx;
            }
        }

        let mut constrained_indices = Vec::new();
        for idx in 0..CHARSET_LEN {
            if constrained_mask & (1u32 << idx) != 0 {
                constrained_indices.push(idx);
            }
        }

        Self {
            fixed_chars,
            cannot_be_at_masks,
            globally_forbidden_mask,
            min_counts,
            exact_counts,
            exact_mask,
            constrained_indices,
        }
    }

    #[inline]
    fn is_globally_forbidden(&self, ch: u8) -> bool {
        self.globally_forbidden_mask & char_mask(ch) != 0
    }

    #[inline]
    fn cannot_be_at(&self, index: usize, ch: u8) -> bool {
        self.cannot_be_at_masks[index] & char_mask(ch) != 0
    }

    #[inline]
    fn exact_count(&self, ch: u8) -> Option<u8> {
        let idx = idx_of(ch);
        if self.exact_mask & (1u32 << idx) != 0 {
            Some(self.exact_counts[idx])
        } else {
            None
        }
    }

    #[inline]
    fn counts_can_still_succeed(&self, counts: &[u8; CHARSET_LEN], remaining_slots: usize) -> bool {
        for &idx in &self.constrained_indices {
            let current = counts[idx] as usize;
            if self.exact_mask & (1u32 << idx) != 0 {
                let exact = self.exact_counts[idx] as usize;
                if current > exact || current + remaining_slots < exact {
                    return false;
                }
            } else {
                let min = self.min_counts[idx] as usize;
                if current + remaining_slots < min {
                    return false;
                }
            }
        }
        true
    }
}

#[inline]
fn idx_of(ch: u8) -> usize {
    match ch {
        b'0' => 0,
        b'1' => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        b'5' => 5,
        b'6' => 6,
        b'7' => 7,
        b'8' => 8,
        b'9' => 9,
        b'+' => 10,
        b'-' => 11,
        b'*' => 12,
        b'/' => 13,
        b'%' => 14,
        b'^' => 15,
        b'=' => 16,
        b'(' => 17,
        b')' => 18,
        b'!' => 19,
        b'[' => 20,
        b']' => 21,
        b'>' => 22,
        b'A' => 23,
        _ => unreachable!("invalid Sumzle character: {ch}"),
    }
}

#[inline]
fn idx_of_char(ch: char) -> Option<usize> {
    if ch.is_ascii() {
        Some(idx_of(ch as u8))
    } else {
        None
    }
}

#[inline]
fn char_mask(ch: u8) -> u32 {
    1u32 << idx_of(ch)
}

#[inline]
const fn is_digit_b(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

#[inline]
const fn is_binary_operator_b(c: u8) -> bool {
    matches!(c, b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'A')
}

#[inline]
const fn is_unary_post_operator_b(c: u8) -> bool {
    c == b'!'
}

#[inline]
const fn is_operator_b(c: u8) -> bool {
    is_binary_operator_b(c) || is_unary_post_operator_b(c)
}

#[inline]
const fn is_open_bracket_b(c: u8) -> bool {
    matches!(c, b'(' | b'[')
}

#[inline]
const fn is_close_bracket_b(c: u8) -> bool {
    matches!(c, b')' | b']')
}

#[inline]
const fn is_main_operator_b(c: u8) -> bool {
    matches!(c, b'=' | b'>')
}

#[inline]
const fn is_end_char_b(c: u8) -> bool {
    is_digit_b(c) || matches!(c, b')' | b']' | b'!')
}

#[inline]
const fn matches_bracket(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']'))
}

#[inline]
fn update_floor_context(ch: u8, ctx: FloorContext) -> FloorContext {
    match ch {
        b'[' => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: false,
        },
        b']' if ctx.in_floor => FloorContext {
            in_floor: false,
            has_slash_in_current_floor: false,
        },
        b'/' if ctx.in_floor => FloorContext {
            in_floor: true,
            has_slash_in_current_floor: true,
        },
        _ => ctx,
    }
}

#[inline]
fn base_candidates(
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
) -> &'static [u8] {
    if floor_ctx.in_floor {
        if floor_ctx.has_slash_in_current_floor {
            FLOOR_WITH_SLASH
        } else {
            FLOOR_NO_SLASH
        }
    } else if main_op_so_far == Some(b'=') {
        if prev_char == Some(b'=') {
            AFTER_EQ_START
        } else {
            AFTER_EQ
        }
    } else if index == 0 {
        FIRST_POSITION
    } else if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            AFTER_DIGIT
        } else if is_binary_operator_b(pc) || is_open_bracket_b(pc) {
            AFTER_BINARY_OR_OPEN
        } else if is_close_bracket_b(pc) || is_unary_post_operator_b(pc) {
            AFTER_CLOSE_OR_FACTORIAL
        } else if is_main_operator_b(pc) {
            AFTER_BINARY_OR_OPEN
        } else {
            DEFAULT_ORDER
        }
    } else {
        DEFAULT_ORDER
    }
}

#[inline]
fn push_filtered(
    slice: &[u8],
    index: usize,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let mut len = 0;
    for &ch in slice {
        if !prepared.is_globally_forbidden(ch) && !prepared.cannot_be_at(index, ch) {
            out[len] = ch;
            len += 1;
        }
    }
    len
}

#[inline]
fn push_filtered_end_chars(
    slice: &[u8],
    index: usize,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let mut len = 0;
    for &ch in slice {
        if is_end_char_b(ch)
            && !prepared.is_globally_forbidden(ch)
            && !prepared.cannot_be_at(index, ch)
        {
            out[len] = ch;
            len += 1;
        }
    }
    len
}

/// Get optimized character order for a given position and context.
#[inline]
fn fill_candidate_chars(
    index: usize,
    prev_char: Option<u8>,
    length: usize,
    main_op_so_far: Option<u8>,
    floor_ctx: FloorContext,
    prepared: &PreparedKnowledge,
    out: &mut [u8; CHARSET_LEN],
) -> usize {
    let fixed = prepared.fixed_chars[index];
    if fixed != NO_CHAR {
        if prepared.is_globally_forbidden(fixed) || prepared.cannot_be_at(index, fixed) {
            return 0;
        }
        out[0] = fixed;
        return 1;
    }

    let ordered = base_candidates(index, prev_char, main_op_so_far, floor_ctx);

    if index == length - 1 && !floor_ctx.in_floor {
        let filtered_len = push_filtered_end_chars(ordered, index, prepared, out);
        if filtered_len > 0 {
            return filtered_len;
        }
        if prev_char.is_some() {
            return push_filtered(END_CHARS, index, prepared, out);
        }
        if index == 0 && length == 1 {
            return push_filtered(LENGTH_ONE_DIGITS, index, prepared, out);
        }
    }

    push_filtered(ordered, index, prepared, out)
}

/// Check if a character can be placed at a given position
#[allow(clippy::too_many_arguments)]
fn can_place_char(
    ch: u8,
    index: usize,
    prev_char: Option<u8>,
    main_op_so_far: Option<u8>,
    char_counts: &[u8; CHARSET_LEN],
    floor_ctx: FloorContext,
    bracket_stack: &[u8],
    prepared: &PreparedKnowledge,
    length: usize,
    current_num_len: u8,
    current_num_value: i64,
    current_num_leading_zero: bool,
) -> bool {
    // Candidates are pre-filtered for fixed-position and cannot-be-at constraints.
    // Only count-dependent constraints remain here.

    // Exact count constraint
    if let Some(exact) = prepared.exact_count(ch) {
        if char_counts[idx_of(ch)] >= exact {
            return false;
        }
    }

    // Floor context constraints
    if floor_ctx.in_floor {
        if ch == b'[' || ch == b'(' || ch == b'A' || ch == b'!' {
            return false;
        }
        if is_operator_b(ch) && ch != b'/' {
            return false;
        }
        if is_main_operator_b(ch) {
            return false;
        }

        if ch == b'/' {
            if floor_ctx.has_slash_in_current_floor {
                return false;
            }
            if !prev_char.is_some_and(is_digit_b) || index == 0 {
                return false;
            }
        } else if ch == b']' {
            if !prev_char.is_some_and(is_digit_b) {
                return false;
            }
            if !floor_ctx.has_slash_in_current_floor {
                return false;
            }
        } else if !is_digit_b(ch) {
            return false;
        }
    }

    // Floor bracket constraints
    if ch == b'[' && floor_ctx.in_floor {
        return false;
    }
    if ch == b']' && !floor_ctx.in_floor {
        return false;
    }
    if ch == b'[' && index >= length.saturating_sub(3) {
        return false;
    }

    // Leading zero check and operand value check
    if is_digit_b(ch) && main_op_so_far != Some(b'=') {
        let digit = (ch - b'0') as i64;
        let continuing_number = prev_char.is_some_and(is_digit_b);
        let new_len = if continuing_number {
            current_num_len as usize + 1
        } else {
            1
        };
        let new_value = if continuing_number {
            current_num_value * 10 + digit
        } else {
            digit
        };
        let leading_zero = if continuing_number {
            current_num_leading_zero
        } else {
            ch == b'0'
        };

        if new_len > 1 && leading_zero {
            return false;
        }
        if new_value > MAX_OPERAND_VALUE {
            return false;
        }
    }

    // First position rules
    if index == 0
        && (is_binary_operator_b(ch)
            || is_close_bracket_b(ch)
            || is_main_operator_b(ch)
            || is_unary_post_operator_b(ch))
    {
        return false;
    }

    // Previous character-based rules
    if let Some(pc) = prev_char {
        if is_digit_b(pc) {
            if is_open_bracket_b(ch) && ch != b'[' {
                return false;
            }
            if ch == b'[' && floor_ctx.in_floor {
                return false;
            }
        } else if is_operator_b(pc) {
            if is_binary_operator_b(ch)
                && !(pc == b'A' && (is_open_bracket_b(ch) || is_digit_b(ch)))
                && !is_unary_post_operator_b(pc)
            {
                return false;
            }
            if is_close_bracket_b(ch) && !is_unary_post_operator_b(pc) {
                return false;
            }
            if is_main_operator_b(ch) && !is_unary_post_operator_b(pc) {
                return false;
            }
            if is_unary_post_operator_b(pc) && (is_digit_b(ch) || is_open_bracket_b(ch)) {
                return false;
            }
        } else if is_open_bracket_b(pc) {
            if pc == b'[' && ch == b'(' {
                return false;
            }
            if is_binary_operator_b(ch) {
                return false;
            }
            if is_close_bracket_b(ch) && !matches_bracket(pc, ch) {
                return false;
            }
            if is_main_operator_b(ch) {
                return false;
            }
            if is_unary_post_operator_b(ch) {
                return false;
            }
        } else if is_close_bracket_b(pc) {
            if is_digit_b(ch) || is_open_bracket_b(ch) {
                return false;
            }
        } else if is_main_operator_b(pc) {
            if pc == b'=' {
                if !is_digit_b(ch) && ch != b'-' {
                    return false;
                }
            } else if is_main_operator_b(ch) || is_close_bracket_b(ch) {
                return false;
            }
        }
    }

    // After main operator =, only digits and minus
    if main_op_so_far == Some(b'=') {
        if !is_digit_b(ch) && ch != b'-' {
            return false;
        }
        if ch == b'-' && prev_char == Some(b'=') && index >= length - 1 {
            return false;
        }
    }

    // Last position rules
    if index == length - 1
        && (is_binary_operator_b(ch) || is_open_bracket_b(ch) || is_main_operator_b(ch))
    {
        return false;
    }

    // Incremental bracket balance check
    let new_stack_len = match ch {
        b'(' | b'[' => bracket_stack.len() + 1,
        b')' | b']' => {
            let Some(&last_open) = bracket_stack.last() else {
                return false;
            };
            if !matches_bracket(last_open, ch) {
                return false;
            }
            bracket_stack.len() - 1
        }
        _ => bracket_stack.len(),
    };

    if index == length - 1 && new_stack_len != 0 {
        return false;
    }

    // Main operator rules
    if is_main_operator_b(ch) {
        if main_op_so_far.is_some() {
            return false;
        }
        if index == 0 || index >= length - 1 {
            return false;
        }
    }

    // Permutation A rules
    if ch == b'A' && !prev_char.is_some_and(|pc| is_digit_b(pc) || is_close_bracket_b(pc)) {
        return false;
    }
    if prev_char == Some(b'A') && !is_digit_b(ch) && !is_open_bracket_b(ch) {
        return false;
    }

    // Factorial ! rules
    if ch == b'!' {
        if prev_char.is_none() {
            return false;
        }
        if let Some(pc) = prev_char {
            if !is_digit_b(pc) && pc != b')' {
                return false;
            }
        }
    }

    true
}

/// The main solver struct
pub struct Solver {
    pub length: usize,
    pub gk: GlobalKnowledge,
    prepared: PreparedKnowledge,
}

impl Solver {
    pub fn new(length: usize, gk: GlobalKnowledge) -> Self {
        let prepared = PreparedKnowledge::new(length, &gk);
        Self {
            length,
            gk,
            prepared,
        }
    }

    /// Solve with single-threaded brute force
    pub fn solve(&self) -> (Vec<String>, u64) {
        let mut results: Vec<String> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = Vec::with_capacity(self.length);

        self.recursive_search(
            0,
            &mut expr,
            None,
            None,
            0,
            &mut char_counts,
            FloorContext::new(),
            &mut bracket_stack,
            &self.prepared,
            0,
            0,
            false,
            &mut results,
            &mut searched_count,
        );

        (results, searched_count)
    }

    #[allow(clippy::too_many_arguments)]
    fn recursive_search(
        &self,
        index: usize,
        expr: &mut [u8],
        prev_char: Option<u8>,
        main_op_so_far: Option<u8>,
        main_op_index: usize,
        char_counts: &mut [u8; CHARSET_LEN],
        floor_ctx: FloorContext,
        bracket_stack: &mut Vec<u8>,
        prepared: &PreparedKnowledge,
        current_num_len: u8,
        current_num_value: i64,
        current_num_leading_zero: bool,
        results: &mut Vec<String>,
        searched_count: &mut u64,
    ) {
        let remaining_slots = self.length - index;
        if !prepared.counts_can_still_succeed(char_counts, remaining_slots) {
            return;
        }

        if main_op_so_far.is_none() {
            let min_needed = if floor_ctx.in_floor {
                2 + if floor_ctx.has_slash_in_current_floor {
                    2
                } else {
                    3
                }
            } else {
                2 + bracket_stack.len()
                    + usize::from(
                        prev_char
                            .is_none_or(|pc| is_binary_operator_b(pc) || is_open_bracket_b(pc)),
                    )
            };
            if remaining_slots < min_needed {
                return;
            }
        } else if floor_ctx.in_floor {
            let min_needed = if floor_ctx.has_slash_in_current_floor {
                2
            } else {
                3
            };
            if remaining_slots < min_needed {
                return;
            }
        }

        if index == self.length {
            *searched_count += 1;

            if main_op_so_far.is_none() {
                return;
            }

            if is_valid_equation_solver_with_main_bytes(
                expr,
                main_op_index,
                main_op_so_far.expect("main operator missing"),
            ) {
                let expr_str = unsafe { std::str::from_utf8_unchecked(expr) };
                results.push(expr_str.to_owned());
            }
            return;
        }

        let mut candidates = [NO_CHAR; CHARSET_LEN];
        let candidate_count = fill_candidate_chars(
            index,
            prev_char,
            self.length,
            main_op_so_far,
            floor_ctx,
            prepared,
            &mut candidates,
        );

        for &ch in &candidates[..candidate_count] {
            if !can_place_char(
                ch,
                index,
                prev_char,
                main_op_so_far,
                char_counts,
                floor_ctx,
                bracket_stack,
                prepared,
                self.length,
                current_num_len,
                current_num_value,
                current_num_leading_zero,
            ) {
                continue;
            }

            expr[index] = ch;
            char_counts[idx_of(ch)] += 1;

            let next_floor_ctx = update_floor_context(ch, floor_ctx);
            let new_main_op = if is_main_operator_b(ch) {
                Some(ch)
            } else {
                main_op_so_far
            };
            let (next_num_len, next_num_value, next_num_leading_zero) = if is_digit_b(ch) {
                if prev_char.is_some_and(is_digit_b) {
                    (
                        current_num_len + 1,
                        current_num_value * 10 + (ch - b'0') as i64,
                        current_num_leading_zero,
                    )
                } else {
                    (1, (ch - b'0') as i64, ch == b'0')
                }
            } else {
                (0, 0, false)
            };

            let mut popped_bracket = None;
            match ch {
                b'(' | b'[' => bracket_stack.push(ch),
                b')' | b']' => popped_bracket = bracket_stack.pop(),
                _ => {}
            }

            self.recursive_search(
                index + 1,
                expr,
                Some(ch),
                new_main_op,
                if is_main_operator_b(ch) {
                    index
                } else {
                    main_op_index
                },
                char_counts,
                next_floor_ctx,
                bracket_stack,
                prepared,
                next_num_len,
                next_num_value,
                next_num_leading_zero,
                results,
                searched_count,
            );

            match ch {
                b'(' | b'[' => {
                    bracket_stack.pop();
                }
                b')' | b']' => {
                    bracket_stack.push(popped_bracket.expect("matching bracket missing"));
                }
                _ => {}
            }

            char_counts[idx_of(ch)] -= 1;
            expr[index] = NO_CHAR;
        }
    }

    /// Get the top-level character branches for parallel execution
    pub fn get_top_level_branches(&self) -> Vec<(char, Option<char>, FloorContext)> {
        let char_counts = [0u8; CHARSET_LEN];
        let bracket_stack: Vec<u8> = Vec::new();

        let mut candidates = [NO_CHAR; CHARSET_LEN];
        let count = fill_candidate_chars(
            0,
            None,
            self.length,
            None,
            FloorContext::new(),
            &self.prepared,
            &mut candidates,
        );

        candidates[..count]
            .iter()
            .copied()
            .filter(|&ch| {
                can_place_char(
                    ch,
                    0,
                    None,
                    None,
                    &char_counts,
                    FloorContext::new(),
                    &bracket_stack,
                    &self.prepared,
                    self.length,
                    0,
                    0,
                    false,
                )
            })
            .map(|ch| {
                let main_op = if is_main_operator_b(ch) {
                    Some(ch as char)
                } else {
                    None
                };
                let floor_ctx = update_floor_context(ch, FloorContext::new());
                (ch as char, main_op, floor_ctx)
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
        let first = first_char as u8;
        if self.prepared.is_globally_forbidden(first)
            || self.prepared.cannot_be_at(0, first)
            || (self.prepared.fixed_chars[0] != NO_CHAR && self.prepared.fixed_chars[0] != first)
        {
            return (Vec::new(), 0);
        }

        let mut results: Vec<String> = Vec::new();
        let mut searched_count: u64 = 0;
        let mut expr: Vec<u8> = vec![NO_CHAR; self.length];
        let mut char_counts = [0u8; CHARSET_LEN];
        let mut bracket_stack: Vec<u8> = Vec::with_capacity(self.length);

        expr[0] = first;
        char_counts[idx_of(first)] += 1;
        match first {
            b'(' | b'[' => bracket_stack.push(first),
            _ => {}
        }

        self.recursive_search(
            1,
            &mut expr,
            Some(first),
            main_op.map(|c| c as u8),
            if main_op.is_some() { 0 } else { 0 },
            &mut char_counts,
            floor_ctx,
            &mut bracket_stack,
            &self.prepared,
            if is_digit_b(first) { 1 } else { 0 },
            if is_digit_b(first) {
                (first - b'0') as i64
            } else {
                0
            },
            first == b'0',
            &mut results,
            &mut searched_count,
        );

        (results, searched_count)
    }
}
