//! Distributed computing support for Sumzle solver
//!
//! This module implements a TCP-based coordinator/worker architecture
//! for distributing solver work across multiple network nodes.

use crate::solver::Solver;
use crate::types::*;
use serde::{Deserialize, Serialize};
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
        gk: GlobalKnowledge,
    },
    /// No more work available
    NoWork,
    /// Shutdown signal
    Shutdown,
    /// Configuration
    Config { length: usize, gk: GlobalKnowledge },
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

        // Also do local work using the same constraints as the main solver.
        let local_branches = branches.clone();
        let local_next_branch = next_branch.clone();
        let local_total_searched = total_searched.clone();
        let local_results = all_results.clone();
        let local_solver = Solver::new(self.solver.length, self.solver.gk.clone());

        let local_handle = std::thread::spawn(move || loop {
            let branch_idx = local_next_branch.fetch_add(1, Ordering::SeqCst) as usize;
            if branch_idx >= local_branches.len() {
                break;
            }
            let (first_char, main_op, floor_ctx) = local_branches[branch_idx];
            let (results, searched) = local_solver.solve_branch(first_char, main_op, floor_ctx);
            local_total_searched.fetch_add(searched, Ordering::Relaxed);
            if !results.is_empty() {
                let mut all = local_results.lock().unwrap();
                all.extend(results);
            }
        });

        // Accept worker connections. Track each spawned handler thread so we can
        // wait for in-flight workers to report before reading the final totals.
        let mut worker_handles = Vec::new();
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
                    let gk_clone = self.solver.gk.clone();

                    worker_handles.push(std::thread::spawn(move || {
                        if let Err(e) = handle_worker(
                            stream,
                            branches_clone,
                            next_branch_clone,
                            total_searched_clone,
                            all_results_clone,
                            length_clone,
                            gk_clone,
                        ) {
                            log::error!("Worker error: {}", e);
                        }
                    }));
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        local_handle.join().unwrap();
        // The accept loop exits once every branch is *assigned*, but remote
        // workers may still be solving their last branch. Join each handler so
        // its final `Results` is recorded before we collect — otherwise results
        // can be non-deterministically lost.
        for handle in worker_handles {
            let _ = handle.join();
        }

        let mut results = all_results.lock().unwrap().clone();
        results.sort_unstable();
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
    gk: GlobalKnowledge,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    loop {
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
                        gk: gk.clone(),
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
            WorkerMessage::Register { .. } => {
                send_message(
                    &mut writer,
                    &CoordinatorMessage::Config {
                        length,
                        gk: gk.clone(),
                    },
                )?;
            }
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

        let reg = WorkerMessage::Register {
            worker_id: self.worker_id.clone(),
            num_threads: self.num_threads,
        };
        send_worker_message(&mut writer, &reg)?;

        loop {
            let msg = read_coordinator_message(&mut reader)?;
            match msg {
                CoordinatorMessage::Config { .. } => {}
                CoordinatorMessage::NoWork | CoordinatorMessage::Shutdown => {
                    // The coordinator has stopped serving this worker and is
                    // closing the connection, so sending `Disconnect` here would
                    // typically fail with BrokenPipe/ConnectionReset. Exit cleanly.
                    break;
                }
                CoordinatorMessage::Work {
                    branch_index,
                    first_char,
                    main_op,
                    floor_ctx,
                    length,
                    gk,
                } => {
                    let solver = Solver::new(length, gk);
                    let (solutions, searched) = solver.solve_branch(first_char, main_op, floor_ctx);

                    let results = WorkerMessage::Results {
                        worker_id: self.worker_id.clone(),
                        solutions,
                        searched_count: searched,
                        branch_index,
                    };
                    send_worker_message(&mut writer, &results)?;
                }
            }

            let req = WorkerMessage::RequestWork {
                worker_id: self.worker_id.clone(),
            };
            send_worker_message(&mut writer, &req)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_local_mode_preserves_constraints() {
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

        let gk = GlobalKnowledge::from_guess_rows(5, &[row]).unwrap();
        let solver = Solver::new(5, gk.clone());
        let (expected_results, expected_searched) = solver.solve();

        let coordinator = Coordinator::new(Solver::new(5, gk), 0);
        let (distributed_results, distributed_searched) = coordinator.run().unwrap();

        assert_eq!(distributed_results, expected_results);
        assert_eq!(distributed_searched, expected_searched);
        assert_eq!(distributed_results, vec!["1+2=3".to_string()]);
    }
}
