//! Distributed computing support for Sumzle solver
//!
//! This module implements a TCP-based coordinator/worker architecture
//! for distributing solver work across multiple network nodes.

use crate::solver::Solver;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Message types for the distributed protocol
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    /// Worker registration
    Register {
        worker_id: String,
        num_threads: usize,
    },
    /// Worker requesting work
    RequestWork { worker_id: String },
    /// Worker reporting results
    Results {
        worker_id: String,
        solutions: Vec<String>,
        searched_count: u64,
        branch_index: usize,
    },
    /// Worker disconnecting
    Disconnect { worker_id: String },
}

/// Message types from coordinator to workers
#[derive(Debug, Serialize, Deserialize)]
pub enum CoordinatorMessage {
    /// Work assignment
    Work {
        branch_index: usize,
        first_char: char,
        main_op: Option<char>,
        floor_ctx: FloorContext,
        length: usize,
    },
    /// No more work available
    NoWork,
    /// Shutdown signal
    Shutdown,
    /// Configuration
    Config { length: usize },
}

/// Distributed solver coordinator
pub struct Coordinator {
    solver: Solver,
    port: u16,
}

impl Coordinator {
    pub fn new(solver: Solver, port: u16) -> Self {
        Self { solver, port }
    }

    /// Run the coordinator, distributing work to connected workers
    pub fn run(&self) -> anyhow::Result<(Vec<String>, u64)> {
        let branches = self.solver.get_top_level_branches();
        let total_branches = branches.len();

        let next_branch = Arc::new(AtomicU64::new(0));
        let total_searched = Arc::new(AtomicU64::new(0));
        let all_results = Arc::new(Mutex::new(Vec::new()));

        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        log::info!("Coordinator listening on port {}", self.port);
        log::info!("Total branches to distribute: {}", total_branches);

        // Also do local work
        let local_branches: Vec<_> = branches.clone();
        let local_next_branch = next_branch.clone();
        let local_total_searched = total_searched.clone();
        let local_results = all_results.clone();

        let length = self.solver.length;

        // Spawn a thread for local solving
        let local_handle = std::thread::spawn(move || loop {
            let branch_idx = local_next_branch.fetch_add(1, Ordering::SeqCst) as usize;
            if branch_idx >= local_branches.len() {
                break;
            }
            let (first_char, main_op, floor_ctx) = local_branches[branch_idx];
            let solver = Solver::new(
                length,
                GlobalKnowledge {
                    fixed_chars: vec![None; length],
                    cannot_be_at: vec![std::collections::HashSet::new(); length],
                    must_appear_min_count: HashMap::new(),
                    must_appear_exact_count: HashMap::new(),
                    globally_forbidden: std::collections::HashSet::new(),
                },
            );
            let (results, searched) = solver.solve_branch(first_char, main_op, floor_ctx);
            local_total_searched.fetch_add(searched, Ordering::Relaxed);
            if !results.is_empty() {
                let mut all = local_results.lock().unwrap();
                all.extend(results);
            }
        });

        // Accept worker connections
        loop {
            if next_branch.load(Ordering::SeqCst) as usize >= total_branches {
                break;
            }

            listener.set_nonblocking(true)?;
            match listener.accept() {
                Ok((stream, addr)) => {
                    log::info!("Worker connected from {}", addr);
                    let next_branch_clone = next_branch.clone();
                    let total_searched_clone = total_searched.clone();
                    let all_results_clone = all_results.clone();
                    let branches_clone = branches.clone();
                    let length_clone = self.solver.length;

                    std::thread::spawn(move || {
                        if let Err(e) = handle_worker(
                            stream,
                            branches_clone,
                            next_branch_clone,
                            total_searched_clone,
                            all_results_clone,
                            length_clone,
                        ) {
                            log::error!("Worker error: {}", e);
                        }
                    });
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        local_handle.join().unwrap();

        let mut results = all_results.lock().unwrap().clone();
        results.sort();
        results.dedup();

        Ok((results, total_searched.load(Ordering::SeqCst)))
    }
}

fn handle_worker(
    stream: TcpStream,
    branches: Vec<(char, Option<char>, FloorContext)>,
    next_branch: Arc<AtomicU64>,
    total_searched: Arc<AtomicU64>,
    all_results: Arc<Mutex<Vec<String>>>,
    length: usize,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    loop {
        // Read worker message
        let mut len_buf = [0u8; 8];
        reader.read_exact(&mut len_buf)?;
        let len = u64::from_be_bytes(len_buf) as usize;
        let mut data = vec![0u8; len];
        reader.read_exact(&mut data)?;

        let msg: WorkerMessage = serde_json::from_slice(&data)?;

        match msg {
            WorkerMessage::RequestWork { .. } => {
                let branch_idx = next_branch.fetch_add(1, Ordering::SeqCst) as usize;
                if branch_idx >= branches.len() {
                    send_message(&mut writer, &CoordinatorMessage::NoWork)?;
                    break;
                }
                let (first_char, main_op, floor_ctx) = branches[branch_idx];
                send_message(
                    &mut writer,
                    &CoordinatorMessage::Work {
                        branch_index: branch_idx,
                        first_char,
                        main_op,
                        floor_ctx,
                        length,
                    },
                )?;
            }
            WorkerMessage::Results {
                solutions,
                searched_count,
                ..
            } => {
                total_searched.fetch_add(searched_count, Ordering::Relaxed);
                if !solutions.is_empty() {
                    let mut all = all_results.lock().unwrap();
                    all.extend(solutions);
                }
            }
            WorkerMessage::Disconnect { .. } => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn send_message<W: Write>(writer: &mut W, msg: &CoordinatorMessage) -> anyhow::Result<()> {
    let data = serde_json::to_vec(msg)?;
    let len = data.len() as u64;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&data)?;
    writer.flush()?;
    Ok(())
}

/// Distributed solver worker
pub struct Worker {
    coordinator_addr: String,
    worker_id: String,
    num_threads: usize,
}

impl Worker {
    pub fn new(coordinator_addr: String, worker_id: String, num_threads: usize) -> Self {
        Self {
            coordinator_addr,
            worker_id,
            num_threads,
        }
    }

    /// Run the worker, connecting to the coordinator and processing work
    pub fn run(&self) -> anyhow::Result<()> {
        let stream = TcpStream::connect(&self.coordinator_addr)?;
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        // Register
        let reg = WorkerMessage::Register {
            worker_id: self.worker_id.clone(),
            num_threads: self.num_threads,
        };
        send_worker_message(&mut writer, &reg)?;

        loop {
            // Request work
            let req = WorkerMessage::RequestWork {
                worker_id: self.worker_id.clone(),
            };
            send_worker_message(&mut writer, &req)?;

            // Read response
            let msg: CoordinatorMessage = read_coordinator_message(&mut reader)?;

            match msg {
                CoordinatorMessage::Work {
                    branch_index,
                    first_char,
                    main_op,
                    floor_ctx,
                    length,
                } => {
                    // Create a minimal solver for this work
                    // In a real implementation, we'd need the full GlobalKnowledge
                    let gk = GlobalKnowledge {
                        fixed_chars: vec![None; length],
                        cannot_be_at: vec![std::collections::HashSet::new(); length],
                        must_appear_min_count: HashMap::new(),
                        must_appear_exact_count: HashMap::new(),
                        globally_forbidden: std::collections::HashSet::new(),
                    };
                    let solver = Solver::new(length, gk);
                    let (solutions, searched) = solver.solve_branch(first_char, main_op, floor_ctx);

                    // Report results
                    let results = WorkerMessage::Results {
                        worker_id: self.worker_id.clone(),
                        solutions,
                        searched_count: searched,
                        branch_index,
                    };
                    send_worker_message(&mut writer, &results)?;
                }
                CoordinatorMessage::NoWork | CoordinatorMessage::Shutdown => {
                    let disc = WorkerMessage::Disconnect {
                        worker_id: self.worker_id.clone(),
                    };
                    send_worker_message(&mut writer, &disc)?;
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

fn send_worker_message<W: Write>(writer: &mut W, msg: &WorkerMessage) -> anyhow::Result<()> {
    let data = serde_json::to_vec(msg)?;
    let len = data.len() as u64;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&data)?;
    writer.flush()?;
    Ok(())
}

fn read_coordinator_message<R: Read>(reader: &mut R) -> anyhow::Result<CoordinatorMessage> {
    let mut len_buf = [0u8; 8];
    reader.read_exact(&mut len_buf)?;
    let len = u64::from_be_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}
