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

pub mod constraints;
pub mod distributed;
pub mod evaluator;
pub mod parallel;
pub mod solver;
pub mod types;
pub mod server;
