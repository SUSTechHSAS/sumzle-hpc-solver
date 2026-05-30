//! Web API server for the Sumzle solver
//!
//! Provides HTTP endpoints for solving puzzles, validating equations,
//! and evaluating expressions using axum. Also serves the frontend
//! static files with SPA fallback support.

use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
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
    /// The character at this position
    pub char: char,
    /// The state: "correct", "present", or "empty"
    pub state: String,
}

impl SolveRow {
    /// Convert an API request row into an internal GuessRow
    fn to_guess_row(&self) -> GuessRow {
        self.tiles
            .iter()
            .map(|t| Tile {
                char: t.char,
                state: parse_tile_state(&t.state),
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

/// Response body for the solve endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveResponse {
    /// All valid solutions found
    pub solutions: Vec<String>,
    /// Solver statistics
    pub stats: SolverStats,
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
// API handlers
// ---------------------------------------------------------------------------

/// POST /api/solve
async fn solve_handler(
    Query(query): Query<SolveQuery>,
    Json(body): Json<SolveRequest>,
) -> Response {
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

    let (results, searched_count) = if query.threads == 1 {
        solver.solve()
    } else {
        let num_threads = if query.threads == 0 {
            num_cpus::get()
        } else {
            query.threads
        };
        let parallel_solver = ParallelSolver::new(solver, Some(num_threads));
        parallel_solver.solve()
    };

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let speed = (searched_count * 1000).checked_div(elapsed_ms).unwrap_or(0);
    let found_count = results.len();

    let response = SolveResponse {
        solutions: results,
        stats: SolverStats {
            searched_count,
            found_count,
            elapsed_ms,
            speed,
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /api/validate
async fn validate_handler(Json(body): Json<ValidateRequest>) -> Response {
    let valid = evaluator::is_valid_equation(&body.equation);
    (StatusCode::OK, Json(ValidateResponse { valid })).into_response()
}

/// POST /api/eval
async fn eval_handler(Json(body): Json<EvalRequest>) -> Response {
    let result = evaluator::evaluate_expression(&body.expression).map(|v| {
        if v == v.floor() {
            (v as i64).to_string()
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
        // Provide a guess row: "1+2=3" with all tiles marked as correct
        let row = SolveRow {
            tiles: vec![
                SolveTile { char: '1', state: "correct".to_string() },
                SolveTile { char: '+', state: "correct".to_string() },
                SolveTile { char: '2', state: "correct".to_string() },
                SolveTile { char: '=', state: "correct".to_string() },
                SolveTile { char: '3', state: "correct".to_string() },
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
        // Provide conflicting constraints: position 0 fixed to '1' and '2'
        let row1 = SolveRow {
            tiles: vec![
                SolveTile { char: '1', state: "correct".to_string() },
                SolveTile { char: '+', state: "empty".to_string() },
                SolveTile { char: '2', state: "empty".to_string() },
                SolveTile { char: '=', state: "empty".to_string() },
                SolveTile { char: '3', state: "empty".to_string() },
            ],
        };
        let row2 = SolveRow {
            tiles: vec![
                SolveTile { char: '2', state: "correct".to_string() },
                SolveTile { char: '+', state: "empty".to_string() },
                SolveTile { char: '2', state: "empty".to_string() },
                SolveTile { char: '=', state: "empty".to_string() },
                SolveTile { char: '4', state: "empty".to_string() },
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
    // JSON deserialization tests: these send raw JSON strings (exactly as
    // the frontend would) instead of constructing Rust structs, so that
    // serde deserialization mismatches are caught at test time.
    // -----------------------------------------------------------------------

    /// Regression test: the frontend sends rows as `[{tiles: [...]}]`,
    /// not as `[[...]]`. This test sends raw JSON matching the frontend
    /// format to ensure the backend can deserialize it correctly.
    #[tokio::test]
    async fn test_solve_frontend_json_format() {
        let mut app = test_app();
        // This is the exact JSON format the frontend sends
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
        assert_eq!(status, HttpStatusCode::OK, "Expected 200 OK, got body: {}", String::from_utf8_lossy(&body));
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.solutions.contains(&"1+2=3".to_string()));
    }

    /// Test that the frontend JSON format with multiple rows works correctly
    #[tokio::test]
    async fn test_solve_frontend_json_multiple_rows() {
        let mut app = test_app();
        // Two non-conflicting rows providing incremental information:
        // Row 1: position 0 = '1' (correct), position 3 = '=' (correct)
        // Row 2: position 1 = '+' (present), other chars absent
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
        assert_eq!(status, HttpStatusCode::OK, "Expected 200 OK, got body: {}", String::from_utf8_lossy(&body));
        let resp: SolveResponse = serde_json::from_slice(&body).unwrap();
        // Verify all solutions contain '1' at position 0 (from correct constraint)
        for sol in &resp.solutions {
            assert!(sol.starts_with('1'), "Solution '{}' should start with '1'", sol);
        }
    }

    /// Test that the frontend JSON format with empty rows works
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

    /// Unit test for SolveRow deserialization: ensures the {tiles: [...]}
    /// format is accepted (this was the root cause of the bug)
    #[test]
    fn test_solve_row_deserialization_frontend_format() {
        let json = r#"{"tiles": [{"char": "1", "state": "correct"}, {"char": "+", "state": "empty"}]}"#;
        let row: SolveRow = serde_json::from_str(json).unwrap();
        assert_eq!(row.tiles.len(), 2);
        assert_eq!(row.tiles[0].char, '1');
        assert_eq!(row.tiles[0].state, "correct");
        assert_eq!(row.tiles[1].char, '+');
        assert_eq!(row.tiles[1].state, "empty");
    }

    /// Unit test for SolveRequest deserialization with frontend format
    #[test]
    fn test_solve_request_deserialization_frontend_format() {
        let json = r#"{"length": 5, "rows": [{"tiles": [{"char": "1", "state": "correct"}]}]}"#;
        let req: SolveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.length, 5);
        assert_eq!(req.rows.len(), 1);
        assert_eq!(req.rows[0].tiles.len(), 1);
        assert_eq!(req.rows[0].tiles[0].char, '1');
    }

    /// Test that parse_tile_state handles all frontend state values
    #[test]
    fn test_parse_tile_state_all_variants() {
        assert_eq!(parse_tile_state("correct"), TileState::Correct);
        assert_eq!(parse_tile_state("present"), TileState::Present);
        assert_eq!(parse_tile_state("empty"), TileState::Empty);
        // Also support shorthand/color variants
        assert_eq!(parse_tile_state("green"), TileState::Correct);
        assert_eq!(parse_tile_state("yellow"), TileState::Present);
        assert_eq!(parse_tile_state("g"), TileState::Correct);
        assert_eq!(parse_tile_state("y"), TileState::Present);
        // Case insensitive
        assert_eq!(parse_tile_state("Correct"), TileState::Correct);
        assert_eq!(parse_tile_state("PRESENT"), TileState::Present);
        assert_eq!(parse_tile_state("Empty"), TileState::Empty);
        // Unknown defaults to Empty
        assert_eq!(parse_tile_state("unknown"), TileState::Empty);
        assert_eq!(parse_tile_state("absent"), TileState::Empty);
    }

    /// Test that SolveRow::to_guess_row converts correctly
    #[test]
    fn test_solve_row_to_guess_row() {
        let solve_row = SolveRow {
            tiles: vec![
                SolveTile { char: '1', state: "correct".to_string() },
                SolveTile { char: '+', state: "present".to_string() },
                SolveTile { char: '2', state: "empty".to_string() },
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
}
