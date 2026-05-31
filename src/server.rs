//! Web API server for the Sumzle solver
//!
//! Provides HTTP endpoints for solving puzzles, validating equations,
//! evaluating expressions, downloading results, and computing character
//! probabilities using axum. Also serves the frontend static files with
//! SPA fallback support.

use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::evaluator;
use crate::parallel::ParallelSolver;
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
}

/// Character probability entry
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// All valid solutions found
    pub solutions: Vec<String>,
    /// Solver statistics
    pub stats: SolverStats,
    /// Character probabilities (how often each char appears across solutions)
    pub char_probabilities: Vec<CharProbability>,
    /// Recommended solution (highest probability score)
    pub recommended: Option<String>,
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

/// Compute character probabilities from a list of solutions.
///
/// For each unique character that appears in any solution, we count how many
/// solutions contain that character and express it as a percentage of total
/// solutions. Results are sorted by probability descending, then alphabetically.
pub fn compute_char_probabilities(solutions: &[String]) -> Vec<CharProbability> {
    if solutions.is_empty() {
        return Vec::new();
    }

    let total = solutions.len() as f64;
    let mut char_counts: HashMap<char, usize> = HashMap::new();

    for sol in solutions {
        let mut seen = std::collections::HashSet::new();
        for ch in sol.chars() {
            if seen.insert(ch) {
                *char_counts.entry(ch).or_insert(0) += 1;
            }
        }
    }

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

    for sol in solutions {
        let mut seen = std::collections::HashSet::new();
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

/// Minimum expression length supported by the solver
const MIN_SOLVE_LENGTH: usize = 3;
/// Maximum expression length supported by the solver
const MAX_SOLVE_LENGTH: usize = 8;
/// Maximum number of threads allowed for a single solve request
const MAX_THREADS: usize = 256;

/// POST /api/solve
async fn solve_handler(
    Query(query): Query<SolveQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
    // Validate length bounds
    if body.length < MIN_SOLVE_LENGTH || body.length > MAX_SOLVE_LENGTH {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "Length must be between {} and {}, got {}",
                    MIN_SOLVE_LENGTH, MAX_SOLVE_LENGTH, body.length
                ),
            }),
        )
            .into_response();
    }

    // Clamp thread count to a reasonable maximum
    let threads = if query.threads > MAX_THREADS {
        MAX_THREADS
    } else {
        query.threads
    };

    // Convert API rows to internal GuessRow format
    let guess_rows: Vec<GuessRow> = body.rows.iter().map(|r| r.to_guess_row()).collect();

    // Build global knowledge from guess rows
    let gk = match GlobalKnowledge::from_guess_rows(body.length, &guess_rows) {
        Ok(gk) => gk,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid constraints: {}", e),
                }),
            )
                .into_response();
        }
    };

    let solver = Solver::new(body.length, gk);

    let start = std::time::Instant::now();

    let (results, searched_count) = if threads == 1 {
        solver.solve()
    } else {
        let num_threads = if threads == 0 {
            num_cpus::get()
        } else {
            threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        parallel_solver.solve()
    };

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let speed = (searched_count * 1000).checked_div(elapsed_ms).unwrap_or(0);
    let found_count = results.len();

    // Compute character probabilities and recommended solution
    let char_probabilities = compute_char_probabilities(&results);
    let recommended = compute_recommended(&results, &char_probabilities);

    let response = SolveResponse {
        solutions: results,
        stats: SolverStats {
            searched_count,
            found_count,
            elapsed_ms,
            speed,
        },
        char_probabilities,
        recommended,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /api/download
async fn download_handler(
    Query(query): Query<DownloadQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
    // Validate length bounds
    if body.length < MIN_SOLVE_LENGTH || body.length > MAX_SOLVE_LENGTH {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "Length must be between {} and {}, got {}",
                    MIN_SOLVE_LENGTH, MAX_SOLVE_LENGTH, body.length
                ),
            }),
        )
            .into_response();
    }

    let threads = num_cpus::get();

    // Convert API rows to internal GuessRow format
    let guess_rows: Vec<GuessRow> = body.rows.iter().map(|r| r.to_guess_row()).collect();

    // Build global knowledge from guess rows
    let gk = match GlobalKnowledge::from_guess_rows(body.length, &guess_rows) {
        Ok(gk) => gk,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid constraints: {}", e),
                }),
            )
                .into_response();
        }
    };

    let solver = Solver::new(body.length, gk);
    let parallel_solver = ParallelSolver::new(solver, Some(threads));

    let start = std::time::Instant::now();
    let (results, searched_count) = parallel_solver.solve();
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let speed = (searched_count * 1000).checked_div(elapsed_ms).unwrap_or(0);
    let found_count = results.len();

    // Compute probabilities and recommendation
    let char_probabilities = compute_char_probabilities(&results);
    let recommended = compute_recommended(&results, &char_probabilities);

    let format = query.format.to_lowercase();
    let timestamp = chrono_now_string();

    match format.as_str() {
        "csv" => {
            let mut csv = String::from("index,expression\n");
            for (i, sol) in results.iter().enumerate() {
                csv.push_str(&format!("{},{}\n", i + 1, sol));
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
            let mut txt = String::new();
            txt.push_str("Sumzle Solver Results\n");
            txt.push_str("=====================\n");
            txt.push_str(&format!("Solutions found: {}\n", found_count));
            txt.push_str(&format!("Expressions searched: {}\n", searched_count));
            txt.push_str(&format!("Time elapsed: {}ms\n", elapsed_ms));
            txt.push_str(&format!("Search speed: {} expr/s\n", speed));
            if let Some(ref rec) = recommended {
                txt.push_str(&format!("Recommended: {}\n", rec));
            }
            txt.push_str("\n--- Solutions ---\n");
            for (i, sol) in results.iter().enumerate() {
                txt.push_str(&format!("{}. {}\n", i + 1, sol));
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
            // Default: JSON format
            let response = SolveResponse {
                solutions: results,
                stats: SolverStats {
                    searched_count,
                    found_count,
                    elapsed_ms,
                    speed,
                },
                char_probabilities,
                recommended,
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
        let req_body = SolveRequest {
            length: 5,
            rows: vec![],
        };
        let (status, _body) = send_request(
            &mut app,
            http::Method::POST,
            "/api/download?format=json",
            serde_json::to_string(&req_body).unwrap(),
        )
        .await;
        assert_eq!(status, HttpStatusCode::OK);
    }

    #[tokio::test]
    async fn test_download_csv() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 3,
            rows: vec![],
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
        // Should have at least 2 lines (header + 1 solution)
        assert!(csv.lines().count() > 1);
    }

    #[tokio::test]
    async fn test_download_txt() {
        let mut app = test_app();
        let req_body = SolveRequest {
            length: 3,
            rows: vec![],
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
        assert!(txt.contains("Solutions found:"));
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
        assert!(resp.error.contains("Length must be between"));
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
    async fn test_solve_length_too_large_rejected() {
        let mut app = test_app();
        let json_body = r#"{"length": 20, "rows": []}"#;
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
