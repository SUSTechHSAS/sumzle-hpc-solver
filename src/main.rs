//! Sumzle Solver CLI

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sumzle_solver::distributed::{Coordinator, Worker};
use sumzle_solver::evaluator;
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::Solver;
use sumzle_solver::types::GlobalKnowledge;
use sumzle_solver::types::*;

#[derive(Parser)]
#[command(name = "sumzle-solver")]
#[command(about = "High-performance Sumzle puzzle solver")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Solve a puzzle from JSON input
    Solve {
        /// JSON input file or stdin
        #[arg(short, long)]
        input: Option<String>,
        /// Number of threads (0 = auto)
        #[arg(short, long, default_value = "0")]
        threads: usize,
        /// Output format: json or text
        #[arg(short, long, default_value = "json")]
        format: String,
    },
    /// Run as distributed coordinator
    Coordinate {
        /// Port to listen on
        #[arg(short, long, default_value = "9876")]
        port: u16,
        /// JSON input file with puzzle definition
        #[arg(short, long)]
        input: String,
    },
    /// Run as distributed worker
    Worker {
        /// Coordinator address (host:port)
        #[arg(short, long)]
        coordinator: String,
        /// Worker ID
        #[arg(short, long, default_value = "worker-1")]
        id: String,
        /// Number of threads
        #[arg(short, long, default_value = "0")]
        threads: usize,
    },
    /// Validate a single equation
    Validate {
        /// The equation to validate
        equation: String,
    },
    /// Evaluate a mathematical expression
    Eval {
        /// The expression to evaluate
        expression: String,
    },
    /// Run benchmarks
    Bench {
        /// Expression lengths to benchmark
        #[arg(short, long, default_value = "6")]
        lengths: Vec<usize>,
    },
    /// Start the web API server
    Serve {
        /// Host to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
}

/// CLI input format
#[derive(Debug, Serialize, Deserialize)]
struct CliInput {
    length: usize,
    rows: Vec<CliRow>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CliRow {
    tiles: Vec<CliTile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CliTile {
    char: char,
    state: String, // "correct", "present", "empty"
}

fn parse_state(s: &str) -> TileState {
    match s.to_lowercase().as_str() {
        "correct" | "green" | "g" => TileState::Correct,
        "present" | "yellow" | "y" => TileState::Present,
        _ => TileState::Empty,
    }
}

fn build_solver_input(input: &CliInput) -> Result<(usize, GlobalKnowledge)> {
    let rows: Vec<GuessRow> = input
        .rows
        .iter()
        .map(|row| {
            row.tiles
                .iter()
                .map(|t| Tile {
                    char: t.char,
                    state: parse_state(&t.state),
                })
                .collect()
        })
        .collect();

    let gk = GlobalKnowledge::from_guess_rows(input.length, &rows)?;
    Ok((input.length, gk))
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Solve {
            input,
            threads,
            format,
        } => {
            let input_str = match input {
                Some(path) => std::fs::read_to_string(path)?,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };

            let cli_input: CliInput = serde_json::from_str(&input_str)?;
            let (length, gk) = build_solver_input(&cli_input)?;

            let start = std::time::Instant::now();

            let (results, searched_count) = if threads == 1 {
                let solver = Solver::new(length, gk);
                solver.solve()
            } else {
                let solver = Solver::new(length, gk);
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
            let solver_result = SolverResult {
                solutions: results,
                stats: SolverStats {
                    searched_count,
                    found_count,
                    elapsed_ms,
                    speed,
                },
            };

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&solver_result)?);
                }
                "text" => {
                    println!("Solutions found: {}", solver_result.stats.found_count);
                    println!(
                        "Expressions searched: {}",
                        solver_result.stats.searched_count
                    );
                    println!("Time: {}ms", solver_result.stats.elapsed_ms);
                    println!("Speed: {} expr/s", solver_result.stats.speed);
                    println!();
                    for (i, sol) in solver_result.solutions.iter().enumerate() {
                        println!("{}. {}", i + 1, sol);
                    }
                }
                _ => {
                    anyhow::bail!("Unknown format: {}", format);
                }
            }
        }

        Commands::Coordinate { port, input } => {
            let input_str = std::fs::read_to_string(input)?;
            let cli_input: CliInput = serde_json::from_str(&input_str)?;
            let (length, gk) = build_solver_input(&cli_input)?;

            let solver = Solver::new(length, gk);
            let coordinator = Coordinator::new(solver, port);
            let (results, searched_count) = coordinator.run()?;

            println!("Distributed solve complete!");
            println!("Solutions found: {}", results.len());
            println!("Expressions searched: {}", searched_count);
            for (i, sol) in results.iter().enumerate() {
                println!("{}. {}", i + 1, sol);
            }
        }

        Commands::Worker {
            coordinator,
            id,
            threads,
        } => {
            let num_threads = if threads == 0 {
                num_cpus::get()
            } else {
                threads
            };
            let worker = Worker::new(coordinator, id, num_threads);
            worker.run()?;
        }

        Commands::Validate { equation } => {
            let valid = evaluator::is_valid_equation(&equation);
            println!("{}", valid);
        }

        Commands::Eval { expression } => match evaluator::evaluate_expression(&expression) {
            Some(val) => println!("{}", val),
            None => println!("Invalid expression"),
        },

        Commands::Bench { lengths } => {
            for &len in &lengths {
                let gk = GlobalKnowledge {
                    fixed_chars: vec![None; len],
                    cannot_be_at: vec![std::collections::HashSet::new(); len],
                    must_appear_min_count: std::collections::HashMap::new(),
                    must_appear_exact_count: std::collections::HashMap::new(),
                    globally_forbidden: std::collections::HashSet::new(),
                };
                let solver = Solver::new(len, gk);

                let start = std::time::Instant::now();
                let (results, searched_count) = solver.solve();
                let elapsed = start.elapsed();

                println!(
                    "Length {}: {} solutions, {} searched, {:?}",
                    len,
                    results.len(),
                    searched_count,
                    elapsed
                );
            }
        }

        Commands::Serve { host, port } => {
            let addr = format!("{}:{}", host, port);
            println!("Starting web server on {}", addr);
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(sumzle_solver::server::run_server(&addr))?;
        }
    }

    Ok(())
}
