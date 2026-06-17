//! Tauri commands exposing the Sumzle solver to the frontend WebView.
//!
//! These commands mirror the HTTP endpoints in `src/server.rs` so the existing
//! frontend `api.ts` only needs a tiny Tauri-aware shim to use them — the
//! request/response shapes are identical.

use serde::{Deserialize, Serialize};
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::server::{
    compute_char_probabilities, compute_recommended, CharProbability, EvalResponse, SolveResponse,
    ValidateResponse,
};
use sumzle_solver::solver::Solver;
use sumzle_solver::types::*;

/// Local copy of `server::char_probabilities_from_counts` (which is private).
/// Builds a sorted `Vec<CharProbability>` from per-character solution counts
/// and the total number of solutions.
fn char_probabilities_from_counts(
    char_counts: impl IntoIterator<Item = (char, usize)>,
    total: usize,
) -> Vec<CharProbability> {
    if total == 0 {
        return Vec::new();
    }
    let total_f = total as f64;
    let mut probs: Vec<CharProbability> = char_counts
        .into_iter()
        .map(|(ch, count)| CharProbability {
            char: ch.to_string(),
            display: display_char(ch),
            count,
            probability: (count as f64 / total_f) * 100.0,
        })
        .collect();
    probs.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.char.cmp(&b.char))
    });
    probs
}

/// Local copy of `server::display_char` (also private).
fn display_char(ch: char) -> String {
    match ch {
        '*' => "×".to_string(),
        '/' => "÷".to_string(),
        _ => ch.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Request types — kept identical to the HTTP API so the frontend doesn't have
// to know whether it is talking to Tauri or to axum.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveTile {
    pub char: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRow {
    pub tiles: Vec<SolveTile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    pub length: usize,
    pub rows: Vec<SolveRow>,
    #[serde(default)]
    pub threads: usize,
    #[serde(default)]
    pub top: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateRequest {
    pub equation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    pub expression: String,
}

// ---------------------------------------------------------------------------
// Helpers — copied from `server.rs` to keep behaviour identical.
// ---------------------------------------------------------------------------

fn parse_tile_state(s: &str) -> TileState {
    match s.to_lowercase().as_str() {
        "correct" | "green" | "g" => TileState::Correct,
        "present" | "yellow" | "y" => TileState::Present,
        _ => TileState::Empty,
    }
}

fn to_guess_row(row: &SolveRow) -> GuessRow {
    row.tiles
        .iter()
        .map(|t| {
            let ch = t.char.chars().next().unwrap_or('\0');
            let state = if ch == '\0' {
                TileState::Empty
            } else {
                parse_tile_state(&t.state)
            };
            Tile { char: ch, state }
        })
        .collect()
}

/// Run the solver on the current thread. Same logic as `solve_handler`.
fn run_solve(req: &SolveRequest) -> Result<SolveResponse, String> {
    const MIN_SOLVE_LENGTH: usize = 3;
    const MAX_THREADS: usize = 256;

    if req.length < MIN_SOLVE_LENGTH {
        return Err(format!(
            "Length must be at least {}, got {}",
            MIN_SOLVE_LENGTH, req.length
        ));
    }

    let threads = if req.threads > MAX_THREADS {
        MAX_THREADS
    } else {
        req.threads
    };
    let top = req.top;

    let guess_rows: Vec<GuessRow> = req.rows.iter().map(to_guess_row).collect();
    let gk = GlobalKnowledge::from_guess_rows(req.length, &guess_rows)
        .map_err(|e| format!("Invalid constraints: {}", e))?;

    let solver = Solver::new(req.length, gk);
    let start = std::time::Instant::now();

    let (results, scores, full_char_probs, searched_count): (
        Vec<String>,
        Vec<f64>,
        Option<Vec<CharProbability>>,
        u64,
    ) = if top > 0 {
        let num_threads = if threads == 0 {
            num_cpus::get()
        } else {
            threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        let (scored, counts, searched) = parallel_solver.solve_top_n_with_counts(top);
        let mut exprs = Vec::with_capacity(scored.len());
        let mut scs = Vec::with_capacity(scored.len());
        for (score, expr) in scored {
            exprs.push(expr);
            scs.push(score);
        }
        let char_probs =
            char_probabilities_from_counts(counts.char_count_pairs(), counts.total as usize);
        (exprs, scs, Some(char_probs), searched)
    } else if threads == 1 {
        let (r, s) = solver.solve();
        (r, Vec::new(), None, s)
    } else {
        let num_threads = if threads == 0 {
            num_cpus::get()
        } else {
            threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        let (r, s) = parallel_solver.solve();
        (r, Vec::new(), None, s)
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let speed = (searched_count * 1000).checked_div(elapsed_ms).unwrap_or(0);
    let found_count = results.len();

    let char_probabilities =
        full_char_probs.unwrap_or_else(|| compute_char_probabilities(&results));
    let recommended = if top > 0 {
        results.first().cloned()
    } else {
        compute_recommended(&results, &char_probabilities)
    };

    Ok(SolveResponse {
        solutions: results,
        stats: sumzle_solver::types::SolverStats {
            searched_count,
            found_count,
            elapsed_ms,
            speed,
        },
        char_probabilities,
        recommended,
        top,
        scores,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Solve a Sumzle puzzle. Runs the (CPU-bound) solver on a blocking task so
/// we don't stall the Tauri async runtime.
#[tauri::command]
async fn solve(req: SolveRequest) -> Result<SolveResponse, String> {
    tokio::task::spawn_blocking(move || run_solve(&req))
        .await
        .map_err(|e| format!("Solver task panicked: {e}"))?
}

#[tauri::command]
fn validate(equation: String) -> ValidateResponse {
    ValidateResponse {
        valid: sumzle_solver::evaluator::is_valid_equation(&equation),
    }
}

#[tauri::command]
fn eval_expression(expression: String) -> EvalResponse {
    let result = sumzle_solver::evaluator::evaluate_expression(&expression).map(|v| {
        if v == v.floor() && v.is_finite() {
            if v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                (v as i64).to_string()
            } else {
                format!("{:.0}", v)
            }
        } else {
            format!("{}", v)
        }
    });
    EvalResponse { result }
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            solve,
            validate,
            eval_expression
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
