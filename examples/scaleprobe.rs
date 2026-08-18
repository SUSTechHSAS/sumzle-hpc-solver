// Isolate search throughput from solution emission and disk I/O.
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::Solver;
use sumzle_solver::types::*;

fn empty_gk(len: usize) -> GlobalKnowledge {
    GlobalKnowledge {
        fixed_chars: vec![None; len],
        cannot_be_at: vec![Default::default(); len],
        must_appear_min_count: HashMap::new(),
        must_appear_exact_count: HashMap::new(),
        globally_forbidden: Default::default(),
    }
}

/// Swallows bytes without touching the OS.
struct Blackhole(u64);
impl Write for Blackhole {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let len: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let threads: Vec<usize> = args.filter_map(|s| s.parse().ok()).collect();
    let threads = if threads.is_empty() {
        vec![1, 2, 4, 8, 16, 32]
    } else {
        threads
    };

    let t0 = Instant::now();
    let solver = Solver::new(len, empty_gk(len));
    println!("L={len} table build: {:.2}s", t0.elapsed().as_secs_f64());

    let mut ps = ParallelSolver::new(solver, Some(1));
    let never = AtomicBool::new(false);
    for t in threads {
        ps.num_threads = t;
        let s = Instant::now();
        let (found, searched) = ps.solve_to_writer(Blackhole(0), &never).unwrap();
        let el = s.elapsed().as_secs_f64();
        println!(
            "t={t:>2} jsonl : {el:>7.2}s  found={found}  searched={searched}  {:>6.0}M expr/s",
            searched as f64 / el / 1e6
        );

        let s2 = Instant::now();
        let (tally, searched2) = ps.solve_tally();
        let el2 = s2.elapsed().as_secs_f64();
        println!(
            "t={t:>2} tally : {el2:>7.2}s  found={tally}  searched={searched2}  {:>6.0}M expr/s",
            searched2 as f64 / el2 / 1e6
        );
    }
}
