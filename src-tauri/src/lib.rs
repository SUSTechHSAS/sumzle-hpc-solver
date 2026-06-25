//! Tauri app entry point and the IPC command layer.
//!
//! The React frontend calls these `#[tauri::command]`s via `invoke()` instead
//! of the web build's `fetch('/api/...')`. All solve/validate/eval logic is the
//! shared, framework-agnostic core in [`sumzle_solver::api`], so the mobile app
//! and the web server compute identical results. There is **no** in-process
//! HTTP server — the solver runs directly on the device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sumzle_solver::api::{self, SolveResponse, SolveRow};
use sumzle_solver::parallel::Progress;
use tauri::ipc::Channel;

/// Live progress streamed by [`solve_progress`], mirroring the web SSE
/// `progress` event payload (`{done, total, phase}`). `phase`: 0 preparing,
/// 1 searching, 2 scoring (top-N), 3 finished.
#[derive(Clone, Serialize)]
struct SolveProgress {
    done: u64,
    total: u64,
    phase: u8,
}

/// Solve a puzzle. Mirrors `POST /api/solve`.
///
/// The search is CPU-bound and can run for seconds, so it runs on a blocking
/// thread to keep the app's event loop responsive.
#[tauri::command]
async fn solve(
    length: usize,
    rows: Vec<SolveRow>,
    threads: usize,
    top: usize,
) -> Result<SolveResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        api::solve_core(length, &rows, threads, top, &Progress::new())
    })
    .await
    .map_err(|e| format!("solve task failed: {e}"))?
}

/// Solve a puzzle while streaming live progress over `on_progress`, then return
/// the final result. Mirrors the SSE `POST /api/solve/progress` endpoint.
///
/// A sampler thread forwards [`Progress`] snapshots to the frontend roughly
/// every 150 ms while the blocking solve runs. If the frontend drops the
/// channel (navigated away), the next `send` fails and we cancel the solve so
/// an abandoned search does not keep the cores busy.
#[tauri::command]
async fn solve_progress(
    length: usize,
    rows: Vec<SolveRow>,
    threads: usize,
    top: usize,
    on_progress: Channel<SolveProgress>,
) -> Result<SolveResponse, String> {
    let progress = Arc::new(Progress::new());
    let done = Arc::new(AtomicBool::new(false));

    // Sampler thread: emit progress snapshots until the solve signals done or
    // the channel closes.
    let sampler_progress = Arc::clone(&progress);
    let sampler_done = Arc::clone(&done);
    let sampler_channel = on_progress.clone();
    let sampler = std::thread::spawn(move || loop {
        let (d, t, phase) = sampler_progress.snapshot();
        if sampler_channel
            .send(SolveProgress {
                done: d,
                total: t,
                phase,
            })
            .is_err()
        {
            // Frontend went away — stop the solve early.
            sampler_progress.cancel();
            break;
        }
        if sampler_done.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    });

    // Run the CPU-bound solve off the event loop.
    let solve_progress = Arc::clone(&progress);
    let result = tauri::async_runtime::spawn_blocking(move || {
        api::solve_core(length, &rows, threads, top, &solve_progress)
    })
    .await
    .map_err(|e| format!("solve task failed: {e}"));

    // Stop the sampler and emit one final snapshot so the bar reaches 100%.
    done.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    let (d, t, phase) = progress.snapshot();
    let _ = on_progress.send(SolveProgress {
        done: d,
        total: t,
        phase,
    });

    result?
}

/// Validate a Sumzle equation. Mirrors `POST /api/validate`.
#[tauri::command]
fn validate(equation: String) -> bool {
    api::validate(&equation)
}

/// Evaluate an expression. Mirrors `POST /api/eval`. Returns `null` (None) for
/// invalid expressions.
#[tauri::command]
fn eval(expression: String) -> Option<String> {
    api::eval(&expression)
}

/// App entry point. On mobile this is invoked by the generated native shim
/// (`#[tauri::mobile_entry_point]`); on desktop it is called from `main()`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            solve,
            solve_progress,
            validate,
            eval
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
