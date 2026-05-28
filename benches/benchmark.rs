//! Benchmarks for the Sumzle solver

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sumzle_solver::constraints::GlobalKnowledge;
use sumzle_solver::parallel::ParallelSolver;
use sumzle_solver::solver::Solver;
use sumzle_solver::types::*;

fn empty_gk(length: usize) -> GlobalKnowledge {
    GlobalKnowledge {
        fixed_chars: vec![None; length],
        cannot_be_at: vec![std::collections::HashSet::new(); length],
        must_appear_min_count: std::collections::HashMap::new(),
        must_appear_exact_count: std::collections::HashMap::new(),
        globally_forbidden: std::collections::HashSet::new(),
    }
}

fn bench_sequential_solve(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_solve");

    for length in [3, 4, 5, 6] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(length), &length, |b, &len| {
            let gk = empty_gk(len);
            b.iter(|| {
                let solver = Solver::new(black_box(len), black_box(gk.clone()));
                solver.solve()
            });
        });
    }
    group.finish();
}

fn bench_parallel_solve(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_solve");

    for length in [5, 6] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(length), &length, |b, &len| {
            let gk = empty_gk(len);
            b.iter(|| {
                let solver = Solver::new(black_box(len), black_box(gk.clone()));
                let ps = ParallelSolver::new(solver, Some(4));
                ps.solve()
            });
        });
    }
    group.finish();
}

fn bench_expression_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("expression_evaluation");

    group.bench_function("simple_arithmetic", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::evaluate_expression(black_box("2*3+4-1"))
        });
    });

    group.bench_function("factorial", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::evaluate_expression(black_box("5!*2"))
        });
    });

    group.bench_function("permutation", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::evaluate_expression(black_box("5A3"))
        });
    });

    group.bench_function("floor", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::evaluate_expression(black_box("[7/2]*3"))
        });
    });

    group.bench_function("complex", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::evaluate_expression(black_box("(2+3)*[7/2]^2"))
        });
    });

    group.finish();
}

fn bench_equation_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("equation_validation");

    group.bench_function("valid_simple", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::is_valid_equation(black_box("1+2=3"))
        });
    });

    group.bench_function("valid_complex", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::is_valid_equation(black_box("5!*2-1=239"))
        });
    });

    group.bench_function("invalid", |b| {
        b.iter(|| {
            sumzle_solver::evaluator::is_valid_equation(black_box("1+2+3"))
        });
    });

    group.finish();
}

fn bench_constraint_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("constraint_processing");

    group.bench_function("single_row", |b| {
        let row: GuessRow = vec![
            Tile { char: '1', state: TileState::Correct },
            Tile { char: '+', state: TileState::Present },
            Tile { char: '2', state: TileState::Empty },
            Tile { char: '=', state: TileState::Correct },
            Tile { char: '3', state: TileState::Empty },
            Tile { char: '0', state: TileState::Empty },
        ];
        b.iter(|| {
            GlobalKnowledge::from_guess_rows(black_box(6), black_box(&[row.clone()]))
        });
    });

    group.bench_function("multi_row", |b| {
        let row1: GuessRow = vec![
            Tile { char: '1', state: TileState::Correct },
            Tile { char: '+', state: TileState::Present },
            Tile { char: '2', state: TileState::Empty },
            Tile { char: '=', state: TileState::Correct },
            Tile { char: '3', state: TileState::Empty },
            Tile { char: '0', state: TileState::Empty },
        ];
        let row2: GuessRow = vec![
            Tile { char: '1', state: TileState::Correct },
            Tile { char: '*', state: TileState::Present },
            Tile { char: '5', state: TileState::Empty },
            Tile { char: '=', state: TileState::Correct },
            Tile { char: '5', state: TileState::Empty },
            Tile { char: '0', state: TileState::Empty },
        ];
        b.iter(|| {
            GlobalKnowledge::from_guess_rows(black_box(6), black_box(&[row1.clone(), row2.clone()]))
        });
    });

    group.finish();
}

fn bench_parallel_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_scaling_length6");

    for threads in [1, 2, 4] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(threads), &threads, |b, &t| {
            let gk = empty_gk(6);
            b.iter(|| {
                let solver = Solver::new(6, gk.clone());
                let ps = ParallelSolver::new(solver, Some(t));
                ps.solve()
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_solve,
    bench_parallel_solve,
    bench_expression_eval,
    bench_equation_validation,
    bench_constraint_processing,
    bench_parallel_scaling,
);

criterion_main!(benches);
