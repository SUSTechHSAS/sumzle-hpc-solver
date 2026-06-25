// Prevents an additional console window on Windows in release builds. Does
// nothing on other platforms; on mobile the entry point is the library's
// `run()` (annotated with `#[tauri::mobile_entry_point]`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sumzle_solver_tauri_lib::run()
}
