//! Sumzle Solver - High-performance equation puzzle solver
//!
//! This library implements a solver for the Sumzle puzzle game,
//! which is a Wordle-like game for mathematical equations.
//!
//! # Features
//! - Single-threaded brute-force solver with extensive pruning
//! - Multi-core parallel solving using Rayon
//! - Distributed computing across network nodes via TCP
//! - Behavioral consistency with the reference JavaScript implementation

// mimalloc as the global allocator (issue #20). The long-running `serve` process
// builds and frees large `Vec<String>` solution sets on every request; glibc's
// malloc retains those freed pages in per-thread arenas, so the resident set
// climbs across solves and never returns to the OS. mimalloc decommits freed
// memory, keeping server RSS steady (`MIMALLOC_PURGE_DELAY=0` returns it
// immediately). Defined here, in the library crate, rather than in `main.rs` so
// the same allocator backs the binary *and* every test/bench binary — which is
// what lets the memory regression test in `server` measure the real allocator.
//
// Gated to non-mobile targets: the rationale is server-RSS-only, and the Tauri
// mobile build (Cargo.toml drops the mimalloc dependency on Android and iOS)
// uses the system allocator — both to avoid cross-compiling mimalloc's bundled
// C with the NDK, and because iOS's malloc integrates with the OS background-
// suspend memory pressure system in ways mimalloc would bypass.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod api;
pub mod constraints;
pub mod distributed;
pub mod evaluator;
pub mod inc_eval;
pub mod parallel;
// HTTP web server (axum). Behind the `server` feature so the Tauri mobile crate
// can depend on this crate with `default-features = false` and never compile
// axum/tokio/tower into the Android binary. The framework-agnostic solve core
// lives in `api` (always compiled).
#[cfg(feature = "server")]
pub mod server;
pub mod solver;
pub mod types;
