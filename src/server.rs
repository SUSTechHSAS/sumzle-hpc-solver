//! Web API server for the Sumzle solver
//!
//! Provides HTTP endpoints for solving puzzles, validating equations,
//! evaluating expressions, downloading results, and computing character
//! probabilities using axum. Also serves the frontend static files with
//! SPA fallback support.

use axum::{
    body::Body,
    extract::Query,
    http::{header, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::evaluator;
use crate::parallel::{ParallelSolver, Progress};
use crate::solver::Solver;
use crate::types::*;

// ---------------------------------------------------------------------------
// API request / response types
// ---------------------------------------------------------------------------

/// A single guess row in the API request, containing tiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRow {
    /// The tiles in this guess row
    pub tiles: Vec<SolveTile>,
}

/// A single tile in an API request row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveTile {
    /// The character at this position (may be empty string for blank tiles)
    pub char: String,
    /// The state: "correct", "present", or "empty"
    pub state: String,
}

impl SolveRow {
    /// Convert an API request row into an internal GuessRow.
    /// Tiles with empty `char` fields are treated as having `Empty` state
    /// since they provide no constraint information.
    fn to_guess_row(&self) -> GuessRow {
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

fn parse_tile_state(s: &str) -> TileState {
    match s.to_lowercase().as_str() {
        "correct" | "green" | "g" => TileState::Correct,
        "present" | "yellow" | "y" => TileState::Present,
        _ => TileState::Empty,
    }
}

/// Request body for the solve endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    /// Expression length to solve for
    pub length: usize,
    /// Guess rows providing constraints (each row has a `tiles` array)
    pub rows: Vec<SolveRow>,
}

/// Query parameters for the solve endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct SolveQuery {
    /// Number of threads (0 = auto, 1 = single-threaded)
    #[serde(default)]
    pub threads: usize,
    /// Return only the top-N highest-scoring solutions (0 = return every
    /// solution). N > 0 uses the memory-bounded two-pass `solve_top_n`.
    #[serde(default)]
    pub top: usize,
}

/// Character probability entry
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

/// Response body for the solve endpoint
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

/// Query parameters for the download endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadQuery {
    /// Download format: "json", "csv", or "txt"
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

/// Request body for the download endpoint.
///
/// Accepts pre-computed solve results, avoiding the need to re-run the solver.
/// The client should send the same `SolveResponse` it received from `/api/solve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    /// All valid solutions found
    pub solutions: Vec<String>,
    /// Solver statistics
    pub stats: SolverStats,
    /// Character probabilities
    #[serde(default)]
    pub char_probabilities: Vec<CharProbability>,
    /// Recommended solution
    #[serde(default)]
    pub recommended: Option<String>,
    /// The top-N value used (0 means all solutions). Preserved so a JSON
    /// download round-trips the `SolveResponse` it was given.
    #[serde(default)]
    pub top: usize,
    /// Per-solution scores aligned with `solutions`. Preserved so top-N JSON
    /// downloads keep the scores from the original solve response.
    #[serde(default)]
    pub scores: Vec<f64>,
}

/// Request body for the validate endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateRequest {
    /// The equation to validate
    pub equation: String,
}

/// Response body for the validate endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResponse {
    /// Whether the equation is valid
    pub valid: bool,
}

/// Request body for the eval endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRequest {
    /// The expression to evaluate
    pub expression: String,
}

/// Response body for the eval endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResponse {
    /// The evaluation result, or null if the expression is invalid
    pub result: Option<String>,
}

/// Generic error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Character probability computation
// ---------------------------------------------------------------------------

/// Map internal solver characters to their display representations
fn display_char(ch: char) -> String {
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

    let mut char_counts: HashMap<char, usize> = HashMap::new();

    let mut seen = std::collections::HashSet::new();
    for sol in solutions {
        seen.clear();
        for ch in sol.chars() {
            if seen.insert(ch) {
                *char_counts.entry(ch).or_insert(0) += 1;
            }
        }
    }

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

    // Build a map from char to probability for quick lookup
    let prob_map: HashMap<char, f64> = probs
        .iter()
        .filter_map(|p| p.char.chars().next().map(|c| (c, p.probability)))
        .collect();

    // Get top 5 characters by probability
    let top_chars: std::collections::HashSet<char> = probs
        .iter()
        .take(5)
        .filter_map(|p| p.char.chars().next())
        .collect();

    let mut best_solution: Option<String> = None;
    let mut best_score: f64 = f64::NEG_INFINITY;

    let mut seen = std::collections::HashSet::new();
    for sol in solutions {
        seen.clear();
        let mut score: f64 = 0.0;

        for ch in sol.chars() {
            if seen.insert(ch) {
                // Add probability score
                score += prob_map.get(&ch).copied().unwrap_or(0.0);
                // Bonus for top 5 characters
                if top_chars.contains(&ch) {
                    score += 50.0;
                }
            }
        }

        if score > best_score {
            best_score = score;
            best_solution = Some(sol.clone());
        }
    }

    best_solution
}

// ---------------------------------------------------------------------------
// API handlers
// ---------------------------------------------------------------------------

/// Minimum expression length supported by the solver. There is intentionally
/// no maximum: the search engine handles any length, so the web API enforces
/// only this lower bound (very large lengths can be slow and memory-hungry).
const MIN_SOLVE_LENGTH: usize = 3;
/// Maximum number of threads allowed for a single solve request
const MAX_THREADS: usize = 256;

/// Validate a solve request and build the solver, shared by `/api/solve`,
/// `/api/solve/progress`, and `/api/solve/stream`. On success returns the
/// prepared `Solver`, the clamped thread count, and the requested top-N value;
/// on failure returns the HTTP status and error message the caller should send.
fn prepare_solve(
    query: &SolveQuery,
    body: &SolveRequest,
) -> Result<(Solver, usize, usize), (StatusCode, String)> {
    // Validate length: only a lower bound is enforced — the search engine
    // supports arbitrary expression lengths, so there is no upper limit.
    if body.length < MIN_SOLVE_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Length must be at least {}, got {}",
                MIN_SOLVE_LENGTH, body.length
            ),
        ));
    }

    // Clamp thread count to a reasonable maximum.
    let threads = query.threads.min(MAX_THREADS);
    let top = query.top;

    // Convert API rows to internal GuessRow format and build global knowledge.
    let guess_rows: Vec<GuessRow> = body.rows.iter().map(|r| r.to_guess_row()).collect();
    let gk = match GlobalKnowledge::from_guess_rows(body.length, &guess_rows) {
        Ok(gk) => gk,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid constraints: {}", e),
            ));
        }
    };

    Ok((Solver::new(body.length, gk), threads, top))
}

/// Run a solve and assemble the full [`SolveResponse`].
///
/// Shared by the plain `/api/solve` handler and the SSE `/api/solve/progress`
/// handler so scores, character probabilities, the recommendation, and
/// `found_count` are computed identically. `progress` receives branch-completion
/// updates during the parallel search (pass a fresh [`Progress`] when nothing is
/// observing it). This is synchronous and CPU-bound; callers on the async
/// runtime that must not block should invoke it inside `spawn_blocking`.
fn run_solve(solver: Solver, threads: usize, top: usize, progress: &Progress) -> SolveResponse {
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
        // reported (the SSE client simply receives the result when it lands).
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
    let speed = (searched_count * 1000).checked_div(elapsed_ms).unwrap_or(0);

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

/// POST /api/solve
async fn solve_handler(
    Query(query): Query<SolveQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
    let (solver, threads, top) = match prepare_solve(&query, &body) {
        Ok(v) => v,
        Err((status, error)) => return (status, Json(ErrorResponse { error })).into_response(),
    };

    // `run_solve` is CPU-bound and can run for seconds; keep it off the async
    // worker threads so it doesn't block the Tokio reactor and stall other
    // requests (the streaming/progress handlers spawn_blocking for the same
    // reason).
    let response = match tokio::task::spawn_blocking(move || {
        run_solve(solver, threads, top, &Progress::new())
    })
    .await
    {
        Ok(response) => response,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "solve task failed".to_string(),
                }),
            )
                .into_response()
        }
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// POST /api/solve/progress
///
/// Same request shape as `/api/solve`, but streams **Server-Sent Events** so the
/// frontend can show a real progress bar (issue #22):
///
/// - `event: progress` with `{"done","total","phase"}` roughly every 150 ms,
///   where `done`/`total` count completed search branches across the threads.
/// - a final `event: result` carrying the full `SolveResponse` JSON.
/// - `event: error` if the solve task fails.
///
/// The CPU-bound solve runs on a blocking thread so it never blocks the async
/// runtime; a ticker task samples the shared [`Progress`] and forwards events.
async fn solve_progress_handler(
    Query(query): Query<SolveQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
    let (solver, threads, top) = match prepare_solve(&query, &body) {
        Ok(v) => v,
        Err((status, error)) => return (status, Json(ErrorResponse { error })).into_response(),
    };

    let progress = Arc::new(Progress::new());
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    // Run the CPU-bound solve off the async runtime.
    let solve_progress = Arc::clone(&progress);
    let mut solve_task =
        tokio::task::spawn_blocking(move || run_solve(solver, threads, top, &solve_progress));

    let progress_for_ticks = Arc::clone(&progress);
    tokio::spawn(async move {
        let progress_event = |p: &Progress| {
            let (done, total, phase) = p.snapshot();
            let data = format!("{{\"done\":{done},\"total\":{total},\"phase\":{phase}}}");
            Event::default().event("progress").data(data)
        };

        let mut ticker = tokio::time::interval(Duration::from_millis(150));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if tx.send(Ok(progress_event(&progress_for_ticks))).await.is_err() {
                        // Client disconnected — tell the worker threads to stop
                        // so the abandoned solve doesn't keep all cores busy.
                        progress_for_ticks.cancel();
                        break;
                    }
                }
                res = &mut solve_task => {
                    // Emit a final progress frame (so the bar reaches 100%) then
                    // the result, then close the stream.
                    let _ = tx.send(Ok(progress_event(&progress_for_ticks))).await;
                    match res {
                        Ok(response) => {
                            let data = serde_json::to_string(&response).unwrap_or_default();
                            let _ = tx.send(Ok(Event::default().event("result").data(data))).await;
                        }
                        Err(_) => {
                            let _ = tx
                                .send(Ok(Event::default().event("error").data("solve task failed")))
                                .await;
                        }
                    }
                    break;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// A [`std::io::Write`] that forwards each write as a chunk over a blocking mpsc
/// sender, bridging the synchronous `solve_to_writer` (run in `spawn_blocking`)
/// into a streaming HTTP body. `blocking_send` applies natural backpressure when
/// the client reads slowly, so server memory stays bounded.
struct ChannelWriter {
    tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, std::io::Error>>,
    /// Flipped when the channel is closed (client disconnected) so the solver's
    /// worker threads can stop searching instead of running to completion.
    cancelled: Arc<AtomicBool>,
}

impl std::io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.tx.blocking_send(Ok(buf.to_vec())).map_err(|_| {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "client disconnected")
        })?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// POST /api/solve/stream
///
/// Streams every solution to the client as chunked NDJSON (one
/// `{"solution":"..."}` per line, `application/x-ndjson`) without materializing
/// the full solution set on the server (issue #21). The frontend pipes the
/// response straight to a file via the File System Access API. The `top` query
/// param is ignored — streaming always returns the complete set.
async fn solve_stream_handler(
    Query(query): Query<SolveQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
    let (solver, threads, _top) = match prepare_solve(&query, &body) {
        Ok(v) => v,
        Err((status, error)) => return (status, Json(ErrorResponse { error })).into_response(),
    };
    let num_threads = if threads == 0 {
        num_cpus::get()
    } else {
        threads
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(16);
    // Shared between the writer (which trips it on a broken pipe) and the solver
    // (whose workers check it before each branch), so an abandoned stream stops
    // searching instead of burning every core to completion.
    let cancelled = Arc::new(AtomicBool::new(false));
    let writer_flag = Arc::clone(&cancelled);
    tokio::task::spawn_blocking(move || {
        // Buffer the per-line NDJSON writes into ~8 KB chunks before they hit the
        // channel. `solve_to_writer` writes one solution per `write` call, which
        // unbuffered would mean thousands of tiny `Vec<u8>` allocations and
        // `blocking_send`s (one per solution) plus an HTTP chunk each. `BufWriter`
        // coalesces them, cutting allocation/contention/chunking overhead. The
        // final `BufWriter` flush is guaranteed by `solve_to_writer`, which calls
        // `flush()?` on the writer before returning (see `parallel.rs`).
        let writer = std::io::BufWriter::new(ChannelWriter {
            tx,
            cancelled: writer_flag,
        });
        let parallel = ParallelSolver::new(solver, Some(num_threads));
        // Errors here are almost always the client disconnecting; the stream is
        // already closing, so there is nothing further to report. Dropping the
        // writer (and its sender) ends the response body.
        let _ = parallel.solve_to_writer(writer, &cancelled);
    });

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/x-ndjson; charset=utf-8".to_string(),
        )],
        Body::from_stream(ReceiverStream::new(rx)),
    )
        .into_response()
}

/// POST /api/download
///
/// Accepts pre-computed solve results and returns them in the requested format
/// (json, csv, or txt). This avoids re-running the solver on the server.
async fn download_handler(
    Query(query): Query<DownloadQuery>,
    Json(body): Json<DownloadRequest>,
) -> Response {
    let found_count = body.solutions.len();
    let searched_count = body.stats.searched_count;
    let elapsed_ms = body.stats.elapsed_ms;
    let speed = body.stats.speed;

    let format = query.format.to_lowercase();
    let timestamp = chrono_now_string();

    match format.as_str() {
        "csv" => {
            use std::fmt::Write;
            let mut csv = String::from("index,expression\n");
            for (i, sol) in body.solutions.iter().enumerate() {
                let _ = writeln!(csv, "{},{}", i + 1, sol);
            }
            let filename = format!("sumzle_solutions_{}.csv", timestamp);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                ],
                csv,
            )
                .into_response()
        }
        "txt" => {
            use std::fmt::Write;
            let mut txt = String::new();
            let _ = writeln!(txt, "Sumzle Solver Results");
            let _ = writeln!(txt, "=====================");
            let _ = writeln!(txt, "Solutions found: {}", found_count);
            let _ = writeln!(txt, "Expressions searched: {}", searched_count);
            let _ = writeln!(txt, "Time elapsed: {}ms", elapsed_ms);
            let _ = writeln!(txt, "Search speed: {} expr/s", speed);
            if let Some(ref rec) = body.recommended {
                let _ = writeln!(txt, "Recommended: {}", rec);
            }
            let _ = writeln!(txt, "\n--- Solutions ---");
            for (i, sol) in body.solutions.iter().enumerate() {
                let _ = writeln!(txt, "{}. {}", i + 1, sol);
            }
            let filename = format!("sumzle_solutions_{}.txt", timestamp);
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "text/plain; charset=utf-8".to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                ],
                txt,
            )
                .into_response()
        }
        _ => {
            // Default: JSON format — return the full SolveResponse, preserving
            // the top-N value and per-solution scores so the download round-trips
            // the response the client received from `/api/solve`.
            let response = SolveResponse {
                solutions: body.solutions,
                stats: body.stats,
                char_probabilities: body.char_probabilities,
                recommended: body.recommended,
                top: body.top,
                scores: body.scores,
            };
            let json_bytes = serde_json::to_string_pretty(&response).unwrap_or_default();
            let filename = format!("sumzle_solutions_{}.json", timestamp);
            (
                StatusCode::OK,
                [
                    (
                        header::CONTENT_TYPE,
                        "application/json; charset=utf-8".to_string(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                ],
                json_bytes,
            )
                .into_response()
        }
    }
}

/// Generate a timestamp string for download filenames
fn chrono_now_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

/// POST /api/validate
async fn validate_handler(Json(body): Json<ValidateRequest>) -> Response {
    let valid = evaluator::is_valid_equation(&body.equation);
    (StatusCode::OK, Json(ValidateResponse { valid })).into_response()
}

/// POST /api/eval
async fn eval_handler(Json(body): Json<EvalRequest>) -> Response {
    let result = evaluator::evaluate_expression(&body.expression).map(|v| {
        if v == v.floor() && v.is_finite() {
            // Format integral floats without narrowing through i64.
            // For values within i64 range, format as integer; otherwise use
            // float formatting to avoid truncation/saturation.
            if v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                (v as i64).to_string()
            } else {
                format!("{:.0}", v)
            }
        } else {
            format!("{}", v)
        }
    });
    (StatusCode::OK, Json(EvalResponse { result })).into_response()
}

// ---------------------------------------------------------------------------
// SPA fallback handler
// ---------------------------------------------------------------------------

/// SPA fallback: for any non-API GET request that doesn't match a static file,
/// serve index.html so that client-side routing works correctly.
async fn spa_fallback() -> Response {
    let frontend_dir = get_frontend_dir();
    let index_path = frontend_dir.join("index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            content,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

// ---------------------------------------------------------------------------
// Frontend directory resolution
// ---------------------------------------------------------------------------

/// Determine the frontend static files directory.
///
/// Resolution order:
/// 1. `STATIC_DIR` environment variable (for custom deployments)
/// 2. `<cwd>/frontend/dist` (for local development)
/// 3. `<exe_dir>/frontend/dist` (for packaged binary next to executable)
fn get_frontend_dir() -> std::path::PathBuf {
    // 1. Check STATIC_DIR env var
    if let Ok(dir) = std::env::var("STATIC_DIR") {
        let path = std::path::PathBuf::from(&dir);
        if path.exists() {
            return path;
        }
    }

    // 2. Check cwd/frontend/dist
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.join("frontend").join("dist");
        if path.exists() {
            return path;
        }
    }

    // 3. Check exe_dir/frontend/dist (for packaged deployments)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let path = exe_dir.join("frontend").join("dist");
            if path.exists() {
                return path;
            }
        }
    }

    std::path::PathBuf::from("frontend/dist")
}

// ---------------------------------------------------------------------------
// Router & server
// ---------------------------------------------------------------------------

/// Create the axum router with all API routes, CORS middleware, and static file serving
pub fn create_router() -> Router {
    let cors = CorsLayer::permissive();

    let api_routes = Router::new()
        .route("/api/solve", post(solve_handler))
        .route("/api/solve/stream", post(solve_stream_handler))
        .route("/api/solve/progress", post(solve_progress_handler))
        .route("/api/download", post(download_handler))
        .route("/api/validate", post(validate_handler))
        .route("/api/eval", post(eval_handler));

    let frontend_dir = get_frontend_dir();
    let has_frontend = frontend_dir.join("index.html").exists();

    if has_frontend {
        log::info!("Serving frontend from: {}", frontend_dir.display());
        println!("Serving frontend from: {}", frontend_dir.display());

        Router::new()
            .merge(api_routes)
            .fallback_service(ServeDir::new(&frontend_dir).fallback(get(spa_fallback)))
            .layer(cors)
    } else {
        log::warn!("No frontend found. API-only mode.");
        println!("No frontend found. Running in API-only mode.");
        Router::new().merge(api_routes).layer(cors)
    }
}

/// Start the web server on the given address
pub async fn run_server(addr: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("Web server listening on {}", addr);
    println!("Web server listening on {}", addr);
    axum::serve(listener, create_router()).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, StatusCode as HttpStatusCode};
    use tower::ServiceExt;

    fn test_app() -> Router {
        create_router()
    }

    async fn send_request(
        app: &mut Router,
        method: http::Method,
        uri: &str,
        body: String,
    ) -> (HttpStatusCode, Vec<u8>) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, body_bytes.to_vec())
    }

    #[tokio::test]
    async fn test_solve_no_constraints() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(!resp.solutions.is_empty());
        assert!(resp.stats.searched_count > 0);
        assert!(resp.stats.found_count > 0);
        // Check new fields
        assert!(!resp.char_probabilities.is_empty());
        assert!(resp.recommended.is_some());
        // Non-top-N mode: `top` echoes 0 and no per-solution scores are sent.
        assert_eq!(resp.top, 0);
        assert!(resp.scores.is_empty());
    }

    #[tokio::test]
    async fn test_solve_top_n_single_thread() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=3",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.top, 3);
        assert!(resp.solutions.len() <= 3);
        // Scores align with solutions and are sorted descending.
        assert_eq!(resp.scores.len(), resp.solutions.len());
        for w in resp.scores.windows(2) {
            assert!(w[0] >= w[1], "scores must be sorted descending");
        }
        // The top-ranked solution is the recommendation.
        assert_eq!(
            resp.recommended.as_deref(),
            resp.solutions.first().map(|s| s.as_str())
        );
    }

    #[tokio::test]
    async fn test_solve_top_n_zero_returns_all() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=0",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.top, 0);
        assert!(resp.scores.is_empty());
        assert!(resp.solutions.len() > 3);
    }

    #[tokio::test]
    async fn test_solve_top_n_single_and_parallel_agree() {
        // The ranking must be identical regardless of thread count — the whole
        // point of routing both paths through `solve_top_n`.
        let req_body = SolveRequest {
            length: 6,
            rows: vec![],
        };
        let body_str = serde_json::to_string(&req_body).unwrap();

        let mut app = test_app();
        let (_, b1) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=10",
            body_str.clone(),
        )
        .await;
        let single: SolveResponse = serde_json::from_slice(&b1).unwrap();

        let mut app = test_app();
        let (_, b2) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=0&top=10",
            body_str,
        )
        .await;
        let parallel: SolveResponse = serde_json::from_slice(&b2).unwrap();

        assert_eq!(single.solutions, parallel.solutions);
        assert_eq!(single.scores, parallel.scores);
    }

    #[tokio::test]
    async fn test_solve_top_n_char_probabilities_use_full_set() {
        // In top-N mode the returned `solutions` are only the ranked top-N, but
        // the character probabilities must reflect the ENTIRE solution set —
        // identical to what a top=0 (all-solutions) solve reports.
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let body_str = serde_json::to_string(&req_body).unwrap();

        let mut app = test_app();
        let (_, all_body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=0",
            body_str.clone(),
        )
        .await;
        let all: SolveResponse = serde_json::from_slice(&all_body).unwrap();

        let mut app = test_app();
        let (_, top_body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=3",
            body_str,
        )
        .await;
        let topn: SolveResponse = serde_json::from_slice(&top_body).unwrap();

        // Only the top-N subset is returned...
        assert_eq!(topn.solutions.len(), 3);
        assert!(all.solutions.len() > 3);
        // ...yet the character probabilities match the full-set probabilities.
        assert!(!topn.char_probabilities.is_empty());
        assert_eq!(
            topn.char_probabilities, all.char_probabilities,
            "top-N char probabilities must be computed over all solutions, not the top-N subset"
        );
    }

    #[tokio::test]
    async fn test_solve_top_n_found_count_is_total() {
        // Issue #19: in top-N mode `found_count` must report the TOTAL number of
        // solutions found across the full search — not the kept top-N subset —
        // mirroring `searched_count` and the character probabilities. The
        // returned `solutions` are still capped at `top`.
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let body_str = serde_json::to_string(&req_body).unwrap();

        let mut app = test_app();
        let (_, all_body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=0",
            body_str.clone(),
        )
        .await;
        let all: SolveResponse = serde_json::from_slice(&all_body).unwrap();

        let mut app = test_app();
        let (_, top_body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=3",
            body_str,
        )
        .await;
        let topn: SolveResponse = serde_json::from_slice(&top_body).unwrap();

        // Only the top-N subset is returned...
        assert_eq!(topn.solutions.len(), 3);
        assert!(all.solutions.len() > 3);
        // ...but the reported count is the full total, identical to a top=0 solve.
        assert_eq!(
            topn.stats.found_count, all.stats.found_count,
            "top-N found_count must be the total solutions found, not the kept top-N"
        );
        assert_eq!(topn.stats.found_count, all.solutions.len());
    }

    /// Pull the concatenated `data:` payload of the first SSE frame whose
    /// `event:` field equals `event`. Tolerant of the optional space after the
    /// field colon. Returns `None` if no such frame is present.
    fn extract_sse_event(text: &str, event: &str) -> Option<String> {
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            let is_match = line
                .strip_prefix("event:")
                .map(|v| v.trim() == event)
                .unwrap_or(false);
            if is_match {
                let mut data = String::new();
                for l in lines.by_ref() {
                    if let Some(rest) = l.strip_prefix("data:") {
                        if !data.is_empty() {
                            data.push('\n');
                        }
                        data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                    } else if l.is_empty() {
                        break;
                    }
                }
                return Some(data);
            }
        }
        None
    }

    #[tokio::test]
    async fn test_solve_stream_endpoint_returns_all_as_ndjson() {
        // Issue #21: /api/solve/stream streams every solution as NDJSON (one
        // {"solution":"..."} per line). The streamed set must equal the full
        // solution set from a normal solve.
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let body_str = serde_json::to_string(&req_body).unwrap();

        let mut app = test_app();
        let (_, all_body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1&top=0",
            body_str.clone(),
        )
        .await;
        let all: SolveResponse = serde_json::from_slice(&all_body).unwrap();

        let mut app = test_app();
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve/stream?threads=2",
            body_str,
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);

        let text = String::from_utf8(body).unwrap();
        let streamed: std::collections::HashSet<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).expect("each line is JSON");
                v["solution"]
                    .as_str()
                    .expect("a string \"solution\" field")
                    .to_string()
            })
            .collect();
        let expected: std::collections::HashSet<String> = all.solutions.iter().cloned().collect();
        assert!(!expected.is_empty());
        assert_eq!(
            streamed, expected,
            "streamed NDJSON must contain exactly the full solution set"
        );
    }

    #[tokio::test]
    async fn test_solve_progress_sse_emits_progress_and_result() {
        // Issue #22: /api/solve/progress streams SSE progress frames followed by
        // a final `result` frame carrying the full SolveResponse.
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let body_str = serde_json::to_string(&req_body).unwrap();

        let mut app = test_app();
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve/progress?threads=2",
            body_str,
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);

        let text = String::from_utf8(body).unwrap();
        assert!(
            text.contains("event: progress") || text.contains("event:progress"),
            "expected at least one progress event, got: {text}"
        );

        let result_data = extract_sse_event(&text, "result").expect("a result SSE frame");
        let resp: SolveResponse = serde_json::from_str(&result_data).unwrap();
        assert!(!resp.solutions.is_empty());
        assert!(resp.stats.found_count > 0);
        assert!(resp.recommended.is_some());
    }

    #[tokio::test]
    async fn test_solve_stream_rejects_bad_length() {
        // The streaming endpoint shares request validation with /api/solve.
        let mut app = test_app();
        let (status, _) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve/stream",
            r#"{"length": 2, "rows": []}"#.to_string(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::BAD_REQUEST);
    }

    /// Regression test for issue #20: the server builds and frees a large
    /// `Vec<String>` of solutions on every request. With the mimalloc global
    /// allocator (defined in `lib.rs`, so it backs this test binary too) the
    /// resident set must stay bounded across many solves — a real leak, or
    /// glibc-style per-arena retention, would climb by hundreds of MB.
    ///
    /// Linux-only (reads `/proc/self/statm`). Each iteration mirrors one
    /// `/api/solve` request: build a fresh solver, produce the full length-6
    /// solution set (~50k strings), then drop it.
    #[cfg(target_os = "linux")]
    #[test]
    fn test_repeated_solves_keep_memory_bounded() {
        fn rss_bytes() -> usize {
            let statm = std::fs::read_to_string("/proc/self/statm").expect("read /proc/self/statm");
            let resident_pages: usize = statm
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .expect("resident pages field in /proc/self/statm");
            // statm reports counts in pages; query the real page size rather than
            // assuming 4 KiB. ARM64 (Graviton, Apple Silicon under Linux) commonly
            // uses 16 KiB or 64 KiB pages, where a hardcoded 4096 underestimates
            // RSS by 4-16x and could mask a regression.
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
            resident_pages * page_size
        }

        let one_solve = || {
            let gk = GlobalKnowledge::from_guess_rows(6, &[]).unwrap();
            let solver = Solver::new(6, gk);
            let (results, _searched) = ParallelSolver::new(solver, None).solve();
            assert!(!results.is_empty());
        };

        // Warm up to a steady-state working set, then measure growth over many
        // more solves. mimalloc reuses/returns memory, so growth stays near zero;
        // if every request's result set were retained instead, 60 length-6 solves
        // would add well over 100 MB.
        for _ in 0..5 {
            one_solve();
        }
        let baseline = rss_bytes();
        for _ in 0..60 {
            one_solve();
        }
        let growth = rss_bytes().saturating_sub(baseline);

        assert!(
            growth < 64 * 1024 * 1024,
            "RSS grew {} MB across 60 solves after warmup — solutions are not being \
             freed across requests (issue #20 regression)",
            growth / (1024 * 1024),
        );
    }

    #[tokio::test]
    async fn test_solve_length_above_old_cap_accepted() {
        // The previous hard cap of 8 has been removed; longer expressions are
        // now accepted. Fixing every tile to a known length-9 equation keeps
        // the search instant and deterministic.
        let mut app = test_app();
        let json_body = r#"{
            "length": 9,
            "rows": [
                {
                    "tiles": [
                        {"char": "1", "state": "correct"},
                        {"char": "0", "state": "correct"},
                        {"char": "+", "state": "correct"},
                        {"char": "2", "state": "correct"},
                        {"char": "+", "state": "correct"},
                        {"char": "3", "state": "correct"},
                        {"char": "=", "state": "correct"},
                        {"char": "1", "state": "correct"},
                        {"char": "5", "state": "correct"}
                    ]
                }
            ]
        }"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(
            status,
            HttpStatusCode::OK,
            "length 9 must be accepted, got body: {}",
            String::from_utf8_lossy(&body)
        );
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            resp.solutions.contains(&"10+2+3=15".to_string()),
            "expected solver to find 10+2+3=15 at length 9, got {:?}",
            resp.solutions
        );
    }

    #[tokio::test]
    async fn test_solve_char_probabilities() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 3,
            rows: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();

        // Check probabilities are sorted descending
        for i in 1..resp.char_probabilities.len() {
            assert!(
                resp.char_probabilities[i - 1].probability
                    >= resp.char_probabilities[i].probability
            );
        }

        // Check all probabilities are between 0 and 100
        for p in &resp.char_probabilities {
            assert!(p.probability > 0.0);
            assert!(p.probability <= 100.0);
        }

        // '=' or '>' must appear in all solutions since all equations need a main operator
        let eq_prob = resp
            .char_probabilities
            .iter()
            .find(|p| p.char == "=")
            .map(|p| p.probability)
            .unwrap_or(0.0);
        let gt_prob = resp
            .char_probabilities
            .iter()
            .find(|p| p.char == ">")
            .map(|p| p.probability)
            .unwrap_or(0.0);
        // At least one of = or > must appear
        assert!(
            eq_prob > 0.0 || gt_prob > 0.0,
            "At least one of = or > should appear in solutions"
        );
    }

    #[tokio::test]
    async fn test_solve_recommended_solution() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();

        // Recommended solution should be one of the solutions
        if let Some(ref rec) = resp.recommended {
            assert!(
                resp.solutions.contains(rec),
                "Recommended solution '{}' should be in solutions list",
                rec
            );
        }
    }

    #[tokio::test]
    async fn test_download_json() {
        let mut app = test_app();
        let req_body = DownloadRequest {
            solutions: vec!["1+2=3".to_string(), "2+1=3".to_string()],
            stats: SolverStats {
                searched_count: 100,
                found_count: 2,
                elapsed_ms: 5,
                speed: 20000,
            },
            char_probabilities: vec![],
            recommended: Some("1+2=3".to_string()),
            top: 2,
            scores: vec![9.5, 8.0],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/download?format=json",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        // The JSON download round-trips the response, including top-N metadata.
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            resp.solutions,
            vec!["1+2=3".to_string(), "2+1=3".to_string()]
        );
        assert_eq!(resp.top, 2);
        assert_eq!(resp.scores, vec![9.5, 8.0]);
    }

    #[tokio::test]
    async fn test_download_csv() {
        let mut app = test_app();
        let req_body = DownloadRequest {
            solutions: vec!["1+2=3".to_string(), "2+1=3".to_string()],
            stats: SolverStats {
                searched_count: 100,
                found_count: 2,
                elapsed_ms: 5,
                speed: 20000,
            },
            char_probabilities: vec![],
            recommended: None,
            top: 0,
            scores: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/download?format=csv",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let csv = String::from_utf8(body).unwrap();
        assert!(csv.starts_with("index,expression\n"));
        assert_eq!(csv.lines().count(), 3); // header + 2 solutions
    }

    #[tokio::test]
    async fn test_download_txt() {
        let mut app = test_app();
        let req_body = DownloadRequest {
            solutions: vec!["1+2=3".to_string()],
            stats: SolverStats {
                searched_count: 50,
                found_count: 1,
                elapsed_ms: 2,
                speed: 25000,
            },
            char_probabilities: vec![],
            recommended: Some("1+2=3".to_string()),
            top: 0,
            scores: vec![],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/download?format=txt",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let txt = String::from_utf8(body).unwrap();
        assert!(txt.contains("Sumzle Solver Results"));
        assert!(txt.contains("Solutions found: 1"));
        assert!(txt.contains("Recommended: 1+2=3"));
    }

    #[tokio::test]
    async fn test_validate_valid() {
        let mut app = test_app();
        let req_body = ValidateRequest {
            equation: "1+2=3".to_string(),
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/validate",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: ValidateResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.valid);
    }

    #[tokio::test]
    async fn test_validate_invalid() {
        let mut app = test_app();
        let req_body = ValidateRequest {
            equation: "1+2=4".to_string(),
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/validate",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: ValidateResponse = serde_json::from_slice(&body).unwrap();
        assert!(!resp.valid);
    }

    #[tokio::test]
    async fn test_eval_simple() {
        let mut app = test_app();
        let req_body = EvalRequest {
            expression: "5!".to_string(),
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/eval",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: EvalResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.result, Some("120".to_string()));
    }

    #[tokio::test]
    async fn test_solve_with_constraints() {
        let mut app = test_app();
        let row = SolveRow {
            tiles: vec![
                SolveTile {
                    char: "1".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "+".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "2".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "=".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "3".to_string(),
                    state: "correct".to_string(),
                },
            ],
        };
        let req_body = SolveRequest {
            length: 5,
            rows: vec![row],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.solutions.contains(&"1+2=3".to_string()));
    }

    #[tokio::test]
    async fn test_solve_conflicting_constraints() {
        let mut app = test_app();
        let row1 = SolveRow {
            tiles: vec![
                SolveTile {
                    char: "1".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "+".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "2".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "=".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "3".to_string(),
                    state: "empty".to_string(),
                },
            ],
        };
        let row2 = SolveRow {
            tiles: vec![
                SolveTile {
                    char: "2".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "+".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "2".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "=".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "4".to_string(),
                    state: "empty".to_string(),
                },
            ],
        };
        let req_body = SolveRequest {
            length: 5,
            rows: vec![row1, row2],
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::BAD_REQUEST);
        let resp: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.error.contains("Invalid constraints"));
    }

    #[tokio::test]
    async fn test_eval_invalid_expression() {
        let mut app = test_app();
        let req_body = EvalRequest {
            expression: "1++2".to_string(),
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/eval",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: EvalResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.result, None);
    }

    // -----------------------------------------------------------------------
    // JSON deserialization tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_solve_frontend_json_format() {
        let mut app = test_app();
        let json_body = r#"{
            "length": 5,
            "rows": [
                {
                    "tiles": [
                        {"char": "1", "state": "correct"},
                        {"char": "+", "state": "correct"},
                        {"char": "2", "state": "correct"},
                        {"char": "=", "state": "correct"},
                        {"char": "3", "state": "correct"}
                    ]
                }
            ]
        }"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(
            status,
            HttpStatusCode::OK,
            "Expected 200 OK, got body: {}",
            String::from_utf8_lossy(&body)
        );
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.solutions.contains(&"1+2=3".to_string()));
    }

    #[tokio::test]
    async fn test_solve_frontend_json_multiple_rows() {
        let mut app = test_app();
        let json_body = r#"{
            "length": 6,
            "rows": [
                {
                    "tiles": [
                        {"char": "1", "state": "correct"},
                        {"char": "+", "state": "present"},
                        {"char": "2", "state": "empty"},
                        {"char": "=", "state": "correct"},
                        {"char": "3", "state": "empty"},
                        {"char": "0", "state": "empty"}
                    ]
                },
                {
                    "tiles": [
                        {"char": "1", "state": "correct"},
                        {"char": "-", "state": "empty"},
                        {"char": "*", "state": "empty"},
                        {"char": "=", "state": "correct"},
                        {"char": "5", "state": "empty"},
                        {"char": "6", "state": "empty"}
                    ]
                }
            ]
        }"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(
            status,
            HttpStatusCode::OK,
            "Expected 200 OK, got body: {}",
            String::from_utf8_lossy(&body)
        );
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        for sol in &resp.solutions {
            assert!(
                sol.starts_with('1'),
                "Solution '{}' should start with '1'",
                sol
            );
        }
    }

    #[tokio::test]
    async fn test_solve_frontend_json_empty_rows() {
        let mut app = test_app();
        let json_body = r#"{"length": 5, "rows": []}"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(!resp.solutions.is_empty());
    }

    #[test]
    fn test_solve_row_deserialization_frontend_format() {
        let json =
            r#"{"tiles": [{"char": "1", "state": "correct"}, {"char": "+", "state": "empty"}]}"#;
        let row: SolveRow = serde_json::from_str(json).unwrap();
        assert_eq!(row.tiles.len(), 2);
        assert_eq!(row.tiles[0].char, "1");
        assert_eq!(row.tiles[0].state, "correct");
        assert_eq!(row.tiles[1].char, "+");
        assert_eq!(row.tiles[1].state, "empty");
    }

    #[test]
    fn test_solve_request_deserialization_frontend_format() {
        let json = r#"{"length": 5, "rows": [{"tiles": [{"char": "1", "state": "correct"}]}]}"#;
        let req: SolveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.length, 5);
        assert_eq!(req.rows.len(), 1);
        assert_eq!(req.rows[0].tiles.len(), 1);
        assert_eq!(req.rows[0].tiles[0].char, "1");
    }

    #[test]
    fn test_parse_tile_state_all_variants() {
        assert_eq!(parse_tile_state("correct"), TileState::Correct);
        assert_eq!(parse_tile_state("present"), TileState::Present);
        assert_eq!(parse_tile_state("empty"), TileState::Empty);
        assert_eq!(parse_tile_state("green"), TileState::Correct);
        assert_eq!(parse_tile_state("yellow"), TileState::Present);
        assert_eq!(parse_tile_state("g"), TileState::Correct);
        assert_eq!(parse_tile_state("y"), TileState::Present);
        assert_eq!(parse_tile_state("Correct"), TileState::Correct);
        assert_eq!(parse_tile_state("PRESENT"), TileState::Present);
        assert_eq!(parse_tile_state("Empty"), TileState::Empty);
        assert_eq!(parse_tile_state("unknown"), TileState::Empty);
        assert_eq!(parse_tile_state("absent"), TileState::Empty);
    }

    #[test]
    fn test_solve_row_to_guess_row() {
        let solve_row = SolveRow {
            tiles: vec![
                SolveTile {
                    char: "1".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "+".to_string(),
                    state: "present".to_string(),
                },
                SolveTile {
                    char: "2".to_string(),
                    state: "empty".to_string(),
                },
            ],
        };
        let guess_row = solve_row.to_guess_row();
        assert_eq!(guess_row.len(), 3);
        assert_eq!(guess_row[0].char, '1');
        assert_eq!(guess_row[0].state, TileState::Correct);
        assert_eq!(guess_row[1].char, '+');
        assert_eq!(guess_row[1].state, TileState::Present);
        assert_eq!(guess_row[2].char, '2');
        assert_eq!(guess_row[2].state, TileState::Empty);
    }

    #[tokio::test]
    async fn test_solve_with_blank_tiles() {
        let mut app = test_app();
        let json_body = r#"{
            "length": 5,
            "rows": [
                {
                    "tiles": [
                        {"char": "", "state": "empty"},
                        {"char": "", "state": "empty"},
                        {"char": "", "state": "empty"},
                        {"char": "", "state": "empty"},
                        {"char": "", "state": "empty"}
                    ]
                }
            ]
        }"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(
            status,
            HttpStatusCode::OK,
            "Expected 200 OK, got body: {}",
            String::from_utf8_lossy(&body)
        );
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(!resp.solutions.is_empty());
    }

    #[tokio::test]
    async fn test_solve_with_partial_blank_tiles() {
        let mut app = test_app();
        let json_body = r#"{
            "length": 5,
            "rows": [
                {
                    "tiles": [
                        {"char": "1", "state": "correct"},
                        {"char": "+", "state": "correct"},
                        {"char": "", "state": "empty"},
                        {"char": "=", "state": "correct"},
                        {"char": "", "state": "empty"}
                    ]
                }
            ]
        }"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(
            status,
            HttpStatusCode::OK,
            "Expected 200 OK, got body: {}",
            String::from_utf8_lossy(&body)
        );
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        for sol in &resp.solutions {
            assert!(
                sol.starts_with("1+"),
                "Solution '{}' should start with '1+'",
                sol
            );
            assert_eq!(
                sol.as_bytes()[3],
                b'=',
                "Solution '{}' should have '=' at pos 3",
                sol
            );
        }
    }

    #[test]
    fn test_solve_tile_empty_char() {
        let json = r#"{"char": "", "state": "empty"}"#;
        let tile: SolveTile = serde_json::from_str(json).unwrap();
        assert_eq!(tile.char, "");
        assert_eq!(tile.state, "empty");
    }

    #[test]
    fn test_solve_row_with_blank_tiles_to_guess_row() {
        let solve_row = SolveRow {
            tiles: vec![
                SolveTile {
                    char: "1".to_string(),
                    state: "correct".to_string(),
                },
                SolveTile {
                    char: "".to_string(),
                    state: "empty".to_string(),
                },
                SolveTile {
                    char: "+".to_string(),
                    state: "present".to_string(),
                },
            ],
        };
        let guess_row = solve_row.to_guess_row();
        assert_eq!(guess_row[0].char, '1');
        assert_eq!(guess_row[0].state, TileState::Correct);
        assert_eq!(guess_row[1].char, '\0');
        assert_eq!(guess_row[1].state, TileState::Empty);
        assert_eq!(guess_row[2].char, '+');
        assert_eq!(guess_row[2].state, TileState::Present);
    }

    #[tokio::test]
    async fn test_solve_zero_length_rejected() {
        let mut app = test_app();
        let json_body = r#"{"length": 0, "rows": []}"#;
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::BAD_REQUEST);
        let resp: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.error.contains("Length must be at least"));
    }

    #[tokio::test]
    async fn test_solve_length_too_small_rejected() {
        let mut app = test_app();
        let json_body = r#"{"length": 2, "rows": []}"#;
        let (status, _body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=1",
            json_body.to_string(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_solve_large_thread_count_clamped() {
        let mut app = test_app();
        let json_body = r#"{"length": 5, "rows": []}"#;
        let (status, _body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/solve?threads=999999",
            json_body.to_string(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
    }

    #[tokio::test]
    async fn test_eval_large_integer() {
        let mut app = test_app();
        let req_body = EvalRequest {
            expression: "10^19".to_string(),
        };
        let (status, body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/eval",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
        let resp: EvalResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.result.is_some());
        let val = resp.result.unwrap();
        assert!(
            !val.contains("-9223372036854775808"),
            "Should not saturate to i64::MIN"
        );
    }

    // -----------------------------------------------------------------------
    // Unit tests for probability and recommendation logic
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_char_probabilities_empty() {
        let probs = compute_char_probabilities(&[]);
        assert!(probs.is_empty());
    }

    #[test]
    fn test_compute_char_probabilities_basic() {
        let solutions = vec![
            "1+2=3".to_string(),
            "2+3=5".to_string(),
            "1+3=4".to_string(),
        ];
        let probs = compute_char_probabilities(&solutions);

        // '=' appears in all solutions
        let eq_prob = probs.iter().find(|p| p.char == "=").unwrap();
        assert_eq!(eq_prob.count, 3);
        assert!((eq_prob.probability - 100.0).abs() < 0.01);

        // '1' appears in 2/3 solutions
        let one_prob = probs.iter().find(|p| p.char == "1").unwrap();
        assert_eq!(one_prob.count, 2);
        assert!((one_prob.probability - 66.67).abs() < 0.1);

        // '*' display mapping
        let star_prob = probs.iter().find(|p| p.char == "*");
        // No * in these solutions
        assert!(star_prob.is_none());
    }

    #[test]
    fn test_compute_char_probabilities_display_mapping() {
        let solutions = vec!["2*3=6".to_string()];
        let probs = compute_char_probabilities(&solutions);
        let star_prob = probs.iter().find(|p| p.char == "*").unwrap();
        assert_eq!(star_prob.display, "×");
    }

    #[test]
    fn test_compute_recommended_empty() {
        let probs = vec![];
        let result = compute_recommended(&[], &probs);
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_recommended_returns_solution() {
        let solutions = vec![
            "1+2=3".to_string(),
            "5*6=30".to_string(),
            "3-1=2".to_string(),
        ];
        let probs = compute_char_probabilities(&solutions);
        let result = compute_recommended(&solutions, &probs);
        assert!(result.is_some());
        assert!(solutions.contains(&result.unwrap()));
    }

    #[test]
    fn test_display_char_mappings() {
        assert_eq!(display_char('*'), "×");
        assert_eq!(display_char('/'), "÷");
        assert_eq!(display_char('+'), "+");
        assert_eq!(display_char('1'), "1");
    }
}
