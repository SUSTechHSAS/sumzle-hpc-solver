//! Framework-agnostic solver API shared by the axum web server (`server.rs`)
//! and the Tauri mobile commands (`src-tauri/`).
//!
//! This module holds the request/response types and the `solve_core` /
//! `validate` / `eval` entry points so every front end computes identical
//! results. It is **always compiled**; `server.rs` (the HTTP glue) sits behind
//! the `server` Cargo feature and builds on top of these items, while the
//! mobile crate depends on this crate with `default-features = false` and calls
//! straight into here — keeping axum/tokio/tower out of the Android binary.

use serde::{Deserialize, Serialize};

use crate::evaluator;
use crate::parallel::{ParallelSolver, Progress};
use crate::solver::Solver;
use crate::types::*;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// A single guess row in a solve request, containing tiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRow {
    /// The tiles in this guess row
    pub tiles: Vec<SolveTile>,
}

/// A single tile in a solve-request row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveTile {
    /// The character at this position (may be empty string for blank tiles)
    pub char: String,
    /// The state: "correct", "present", or "empty"
    pub state: String,
}

impl SolveRow {
    /// Convert a request row into an internal [`GuessRow`].
    /// Tiles with empty `char` fields are treated as having `Empty` state
    /// since they provide no constraint information.
    pub(crate) fn to_guess_row(&self) -> GuessRow {
        self.tiles
            .iter()
            .map(|t| {
                let ch = t.char.chars().next().unwrap_or('\0');
                let state = if ch == '\0' {
                    // Blank tile (empty char) always means no constraint
                    TileState::Empty
                } else {
                    parse_tile_state(&t.state)
                };
                Tile { char: ch, state }
            })
            .collect()
    }
}

pub(crate) fn parse_tile_state(s: &str) -> TileState {
    match s.to_lowercase().as_str() {
        "correct" | "green" | "g" => TileState::Correct,
        "present" | "yellow" | "y" => TileState::Present,
        _ => TileState::Empty,
    }
}

/// Request body for a solve operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    /// Expression length to solve for
    pub length: usize,
    /// Guess rows providing constraints (each row has a `tiles` array)
    pub rows: Vec<SolveRow>,
}

/// Character probability entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharProbability {
    /// The character
    pub char: String,
    /// Display character (e.g. × for *, ÷ for /)
    pub display: String,
    /// Number of solutions containing this character
    pub count: usize,
    /// Percentage of solutions containing this character
    pub probability: f64,
}

/// Response body for a solve operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveResponse {
    /// Valid solutions found — every solution, or the top-N ranked by score
    /// (descending) when `top` was requested.
    pub solutions: Vec<String>,
    /// Solver statistics
    pub stats: SolverStats,
    /// Character probabilities (how often each char appears across solutions)
    pub char_probabilities: Vec<CharProbability>,
    /// Recommended solution (highest probability score)
    pub recommended: Option<String>,
    /// The top-N value used (0 means all solutions were returned).
    #[serde(default)]
    pub top: usize,
    /// Score of each entry in `solutions`, aligned by index. Empty unless
    /// `top` > 0 (in which case `solutions` is sorted by it, descending).
    #[serde(default)]
    pub scores: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Character probability computation
// ---------------------------------------------------------------------------

/// Map internal solver characters to their display representations.
pub(crate) fn display_char(ch: char) -> String {
    match ch {
        '*' => "×".to_string(),
        '/' => "÷".to_string(),
        _ => ch.to_string(),
    }
}

/// Build the sorted [`CharProbability`] list from per-character solution counts
/// and the total number of solutions.
///
/// `char_counts` yields `(char, count)` pairs where `count` is how many of the
/// `total` solutions contain that character. Shared by
/// [`compute_char_probabilities`] (which derives the counts from a materialized
/// solution list) and the top-N path (which gets them from the solver's
/// `CountSink`, computed over the full solution set rather than the kept
/// top-N). Sorted by probability descending, then character ascending.
fn char_probabilities_from_counts(
    char_counts: impl IntoIterator<Item = (char, usize)>,
    total: usize,
) -> Vec<CharProbability> {
    if total == 0 {
        return Vec::new();
    }

    let total = total as f64;
    let mut probs: Vec<CharProbability> = char_counts
        .into_iter()
        .map(|(ch, count)| CharProbability {
            char: ch.to_string(),
            display: display_char(ch),
            count,
            probability: (count as f64 / total) * 100.0,
        })
        .collect();

    // Sort by probability descending, then by char ascending
    probs.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.char.cmp(&b.char))
    });

    probs
}

/// Compute character probabilities from a list of solutions.
///
/// For each unique character that appears in any solution, we count how many
/// solutions contain that character and express it as a percentage of total
/// solutions. Results are sorted by probability descending, then alphabetically.
pub fn compute_char_probabilities(solutions: &[String]) -> Vec<CharProbability> {
    if solutions.is_empty() {
        return Vec::new();
    }

    // Sumzle expressions are pure ASCII (digits + operators), so we can count
    // with stack-allocated arrays indexed by byte value instead of hashing —
    // no heap allocation in this hot, on-device loop. `counts` accumulates how
    // many solutions contain each character; `seen` dedupes within one solution
    // so a repeated character still counts once. Any non-ASCII byte (which a
    // valid expression never contains) is simply ignored.
    let mut counts = [0usize; 128];
    let mut seen = [false; 128];

    for sol in solutions {
        seen.fill(false);
        for b in sol.bytes() {
            let i = b as usize;
            if i < 128 && !seen[i] {
                seen[i] = true;
                counts[i] += 1;
            }
        }
    }

    let char_counts = (0u8..128)
        .filter(|&b| counts[b as usize] > 0)
        .map(|b| (b as char, counts[b as usize]));
    char_probabilities_from_counts(char_counts, solutions.len())
}

/// Compute the recommended solution based on character probability scores.
///
/// The scoring algorithm:
/// 1. Base score = sum of probabilities of all unique characters in the solution
/// 2. Bonus: +50 points per character from the top 5 most probable characters
///    that appears in the solution
///
/// The solution with the highest score is recommended.
pub fn compute_recommended(solutions: &[String], probs: &[CharProbability]) -> Option<String> {
    if solutions.is_empty() {
        return None;
    }

    // ASCII lookup tables (Sumzle characters are ASCII): the probability of
    // each character and whether it is among the top-5 most probable. Indexing
    // by byte value avoids hashing on every character of every solution in this
    // on-device hot loop. Characters absent from `probs` keep probability 0.0.
    let mut prob_table = [0f64; 128];
    let mut is_top = [false; 128];
    for (rank, p) in probs.iter().enumerate() {
        if let Some(b) = p.char.bytes().next() {
            let i = b as usize;
            if i < 128 {
                prob_table[i] = p.probability;
                if rank < 5 {
                    is_top[i] = true;
                }
            }
        }
    }

    let mut best_solution: Option<&String> = None;
    let mut best_score: f64 = f64::NEG_INFINITY;

    let mut seen = [false; 128];
    for sol in solutions {
        seen.fill(false);
        let mut score: f64 = 0.0;

        for b in sol.bytes() {
            let i = b as usize;
            if i < 128 && !seen[i] {
                seen[i] = true;
                // Add probability score, plus a bonus for top-5 characters.
                score += prob_table[i];
                if is_top[i] {
                    score += 50.0;
                }
            }
        }

        if score > best_score {
            best_score = score;
            best_solution = Some(sol);
        }
    }

    best_solution.cloned()
}

// ---------------------------------------------------------------------------
// Core solve / validate / eval
// ---------------------------------------------------------------------------

/// Minimum expression length supported by the solver. There is intentionally
/// no maximum: the search engine handles any length, so the API enforces only
/// this lower bound (very large lengths can be slow and memory-hungry).
pub const MIN_SOLVE_LENGTH: usize = 3;
/// Maximum number of threads allowed for a single solve request.
pub const MAX_THREADS: usize = 256;

/// Validate a solve request and build the [`Solver`].
///
/// Returns the prepared `Solver`, the clamped thread count, and the requested
/// top-N value, or a human-readable error message (the caller decides how to
/// surface it — e.g. an HTTP 400 or a rejected Tauri command). The axum
/// streaming handler calls this directly because it needs the raw `Solver`;
/// most callers want [`solve_core`], which also runs the search.
pub fn prepare_solve(
    length: usize,
    rows: &[SolveRow],
    threads: usize,
    top: usize,
) -> Result<(Solver, usize, usize), String> {
    prepare_solve_with_budget(
        length,
        rows,
        threads,
        top,
        crate::solver::DEFAULT_MEMORY_BUDGET,
    )
}

/// Like [`prepare_solve`], but with an explicit cap on the memory the RHS
/// value index may use (see [`Solver::with_memory_budget`]).
///
/// The index only accelerates the search — anything that does not fit falls
/// back to the recursive search with identical results — so this is a ceiling
/// on the solver's length-dependent memory rather than a requirement. Callers
/// running many concurrent solves on a small host can lower it; `0` disables
/// indexing entirely.
pub fn prepare_solve_with_budget(
    length: usize,
    rows: &[SolveRow],
    threads: usize,
    top: usize,
    memory_budget: usize,
) -> Result<(Solver, usize, usize), String> {
    if length < MIN_SOLVE_LENGTH {
        return Err(format!(
            "Length must be at least {}, got {}",
            MIN_SOLVE_LENGTH, length
        ));
    }

    // Clamp thread count to a reasonable maximum.
    let threads = threads.min(MAX_THREADS);

    // Convert request rows to internal GuessRow format and build global knowledge.
    let guess_rows: Vec<GuessRow> = rows.iter().map(|r| r.to_guess_row()).collect();
    let gk = GlobalKnowledge::from_guess_rows(length, &guess_rows)
        .map_err(|e| format!("Invalid constraints: {}", e))?;

    Ok((
        Solver::with_memory_budget(length, gk, memory_budget),
        threads,
        top,
    ))
}

/// Run a solve and assemble the full [`SolveResponse`].
///
/// Shared by the plain solve path and the streaming-progress path so scores,
/// character probabilities, the recommendation, and `found_count` are computed
/// identically. `progress` receives branch-completion updates during the
/// parallel search (pass a fresh [`Progress`] when nothing is observing it).
/// This is synchronous and CPU-bound; async callers must invoke it inside a
/// blocking task (`spawn_blocking`) so it doesn't stall the runtime.
pub fn run_solve(solver: Solver, threads: usize, top: usize, progress: &Progress) -> SolveResponse {
    let start = std::time::Instant::now();

    // `scores` is populated (aligned with `solutions`) only in top-N mode.
    // `full_char_probs` is `Some` only in top-N mode, where the character
    // probabilities must be computed over the *entire* solution set — the
    // returned `solutions` are just the ranked top-N subset, so their own
    // frequencies would be misleading. In the other modes `results` already
    // holds every solution, so the probabilities are derived from it below.
    //
    // `found_count` is the total number of solutions found across the *full*
    // search. In top-N mode `results` is only the ranked top-N subset, so its
    // length would under-report the total — the count comes from the pass-1
    // `CountSink::total` instead (matching how `searched_count` and the
    // character probabilities already report over the full set). See issue #19.
    let (results, scores, full_char_probs, searched_count, found_count) = if top > 0 {
        // Top-N: bounded-memory two-pass scoring through the shared, tested
        // `solve_top_n` — identical scoring to `compute_recommended`. Routing
        // both the single- and multi-threaded cases through it keeps the
        // ranking from diverging between them.
        let num_threads = if threads == 0 {
            num_cpus::get()
        } else {
            threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        let (scored, counts, searched) =
            parallel_solver.solve_top_n_with_counts_progress(top, progress);
        let mut exprs = Vec::with_capacity(scored.len());
        let mut scs = Vec::with_capacity(scored.len());
        for (score, expr) in scored {
            exprs.push(expr);
            scs.push(score);
        }
        // Character probabilities over ALL solutions (from pass-1 counts), not
        // just the kept top-N.
        let char_probs =
            char_probabilities_from_counts(counts.char_count_pairs(), counts.total as usize);
        // Total solutions found is the full-set count, not the kept top-N.
        (
            exprs,
            scs,
            Some(char_probs),
            searched,
            counts.total as usize,
        )
    } else if threads == 1 {
        // True single-threaded path: no branch partitioning, so progress is not
        // reported (the observer simply receives the result when it lands).
        let (r, s) = solver.solve();
        let found = r.len();
        (r, Vec::new(), None, s, found)
    } else {
        let num_threads = if threads == 0 {
            num_cpus::get()
        } else {
            threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        let (r, s) = parallel_solver.solve_with_progress(progress);
        let found = r.len();
        (r, Vec::new(), None, s, found)
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let speed = (searched_count * 1000)
        .checked_div(elapsed_ms.max(1))
        .unwrap_or(0);

    // Character probabilities: in top-N mode they were computed over the full
    // solution set above; otherwise derive them from the complete `results`.
    let char_probabilities =
        full_char_probs.unwrap_or_else(|| compute_char_probabilities(&results));
    // In top-N mode `results` is already ranked by (full-set) score, so the
    // first entry is the recommendation; otherwise scan for the best.
    let recommended = if top > 0 {
        results.first().cloned()
    } else {
        compute_recommended(&results, &char_probabilities)
    };

    SolveResponse {
        solutions: results,
        stats: SolverStats {
            searched_count,
            found_count,
            elapsed_ms,
            speed,
        },
        char_probabilities,
        recommended,
        top,
        scores,
    }
}

/// Build and run a solve in one call, returning the assembled [`SolveResponse`].
///
/// Folds [`prepare_solve`] (validation + solver construction) and [`run_solve`]
/// (the CPU-bound search + scoring). This is the single entry point used by the
/// Tauri mobile commands; the axum handlers compose `prepare_solve` and
/// `run_solve` separately because the streaming handler needs the raw `Solver`.
pub fn solve_core(
    length: usize,
    rows: &[SolveRow],
    threads: usize,
    top: usize,
    progress: &Progress,
) -> Result<SolveResponse, String> {
    let (solver, threads, top) = prepare_solve(length, rows, threads, top)?;
    Ok(run_solve(solver, threads, top, progress))
}

/// Whether `equation` is a valid Sumzle equation. Core behind `/api/validate`
/// and the Tauri `validate` command.
pub fn validate(equation: &str) -> bool {
    evaluator::is_valid_equation(equation)
}

/// Evaluate `expression`, formatting the result the way the API and UI expect:
/// integral finite values render without a decimal point (e.g. `12`, not `12`),
/// values outside `i64` range fall back to `{:.0}`, and non-integral values use
/// the default float formatting. Returns `None` for invalid expressions. Core
/// behind `/api/eval` and the Tauri `eval` command.
pub fn eval(expression: &str) -> Option<String> {
    evaluator::evaluate_expression(expression).map(|v| {
        if v == v.floor() && v.is_finite() {
            // Format integral floats without narrowing through i64.
            // For values within i64 range, format as integer; otherwise use
            // float formatting to avoid truncation/saturation.
            if v >= i64::MIN as f64 && v < i64::MAX as f64 {
                (v as i64).to_string()
            } else {
                format!("{:.0}", v)
            }
        } else {
            format!("{}", v)
        }
    })
}
