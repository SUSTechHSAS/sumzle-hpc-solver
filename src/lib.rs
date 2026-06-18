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
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod constraints;
pub mod distributed;
pub mod evaluator;
pub mod parallel;
pub mod server;
pub mod solver;
pub mod types;
