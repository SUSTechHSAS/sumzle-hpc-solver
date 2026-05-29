//! Web API server for the Sumzle solver
//!
//! Provides HTTP endpoints for solving puzzles, validating equations,
//! and evaluating expressions using axum.

use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
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

/// Request body for the solve endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    /// Expression length to solve for
    pub length: usize,
    /// Guess rows providing constraints
    pub rows: Vec<GuessRow>,
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
    // Build global knowledge from guess rows
    let gk = match GlobalKnowledge::from_guess_rows(body.length, &body.rows) {
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
// Router & server
// ---------------------------------------------------------------------------

/// Create the axum router with all API routes, CORS middleware, and static file serving
pub fn create_router() -> Router {
    let cors = CorsLayer::permissive();

    let api_routes = Router::new()
        .route("/api/solve", post(solve_handler))
        .route("/api/validate", post(validate_handler))
        .route("/api/eval", post(eval_handler));

    // Try to serve frontend static files from frontend/dist directory
    let frontend_dir = std::env::current_dir()
        .map(|p| p.join("frontend").join("dist"))
        .unwrap_or_else(|_| std::path::PathBuf::from("frontend/dist"));

    if frontend_dir.exists() {
        Router::new()
            .merge(api_routes)
            .fallback_service(ServeDir::new(&frontend_dir))
            .layer(cors)
    } else {
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
        let row: GuessRow = vec![
            Tile {
                char: '1',
                state: TileState::Correct,
            },
            Tile {
                char: '+',
                state: TileState::Correct,
            },
            Tile {
                char: '2',
                state: TileState::Correct,
            },
            Tile {
                char: '=',
                state: TileState::Correct,
            },
            Tile {
                char: '3',
                state: TileState::Correct,
            },
        ];
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
        let row1: GuessRow = vec![
            Tile {
                char: '1',
                state: TileState::Correct,
            },
            Tile {
                char: '+',
                state: TileState::Empty,
            },
            Tile {
                char: '2',
                state: TileState::Empty,
            },
            Tile {
                char: '=',
                state: TileState::Empty,
            },
            Tile {
                char: '3',
                state: TileState::Empty,
            },
        ];
        let row2: GuessRow = vec![
            Tile {
                char: '2',
                state: TileState::Correct,
            },
            Tile {
                char: '+',
                state: TileState::Empty,
            },
            Tile {
                char: '2',
                state: TileState::Empty,
            },
            Tile {
                char: '=',
                state: TileState::Empty,
            },
            Tile {
                char: '4',
                state: TileState::Empty,
            },
        ];
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
}
