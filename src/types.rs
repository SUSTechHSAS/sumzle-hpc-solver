//! Core types for the Sumzle solver

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Valid characters in a Sumzle expression
pub const VALID_CHARS: &[u8] = b"0123456789+-*/%^=()![]>A";

/// Maximum value for a single operand
pub const MAX_OPERAND_VALUE: i64 = 30;

/// Maximum factorial input
pub const MAX_FACTORIAL: u64 = 12;

/// Maximum permutation parameter
pub const MAX_PERMUTATION: u64 = 10;

/// Tile state for constraint feedback (Wordle-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileState {
    /// Character is correct and in the right position (green)
    Correct,
    /// Character exists in the answer but in a different position (yellow)
    Present,
    /// Character is not in the answer (gray)
    Empty,
}

/// A single tile in a guess row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub char: char,
    pub state: TileState,
}

/// A guess row (one attempt at solving)
pub type GuessRow = Vec<Tile>;

/// Preprocessed global knowledge from all constraint rows
#[derive(Debug, Clone)]
pub struct GlobalKnowledge {
    /// Characters fixed at each position (from "correct" tiles)
    pub fixed_chars: Vec<Option<char>>,
    /// Characters that cannot appear at each position
    pub cannot_be_at: Vec<HashSet<char>>,
    /// Minimum count required for each character
    pub must_appear_min_count: HashMap<char, usize>,
    /// Exact count required for each character
    pub must_appear_exact_count: HashMap<char, usize>,
    /// Characters that are globally forbidden
    pub globally_forbidden: HashSet<char>,
}

/// Floor bracket context during search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloorContext {
    pub in_floor: bool,
    pub has_slash_in_current_floor: bool,
}

impl FloorContext {
    pub const fn new() -> Self {
        Self {
            in_floor: false,
            has_slash_in_current_floor: false,
        }
    }
}

impl Default for FloorContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Solver input specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverInput {
    /// Expression length
    pub length: usize,
    /// Guess rows with constraints
    pub rows: Vec<GuessRow>,
}

/// Solver statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverStats {
    /// Total expressions searched
    pub searched_count: u64,
    /// Number of solutions found
    pub found_count: usize,
    /// Time elapsed in milliseconds
    pub elapsed_ms: u64,
    /// Search speed (expressions/second)
    pub speed: u64,
}

/// Solver result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverResult {
    /// All valid solutions found
    pub solutions: Vec<String>,
    /// Solver statistics
    pub stats: SolverStats,
}

/// Character classification helpers
#[inline]
pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

#[inline]
pub fn is_binary_operator(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '%' | '^' | 'A')
}

#[inline]
pub fn is_unary_post_operator(c: char) -> bool {
    c == '!'
}

#[inline]
pub fn is_operator(c: char) -> bool {
    is_binary_operator(c) || is_unary_post_operator(c)
}

#[inline]
pub fn is_open_bracket(c: char) -> bool {
    matches!(c, '(' | '[')
}

#[inline]
pub fn is_close_bracket(c: char) -> bool {
    matches!(c, ')' | ']')
}

#[inline]
pub fn is_main_operator(c: char) -> bool {
    matches!(c, '=' | '>')
}

#[inline]
pub fn get_matching_bracket(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        _ => None,
    }
}
