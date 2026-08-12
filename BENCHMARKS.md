# Benchmark Results

All benchmarks run single-threaded in release mode.

## Test Environment
- **Date**: 2026-06-12
- **Cores**: 2 (sandbox)
- **Build**: `cargo build --release`
- **Rust**: 1.96.0

## Solver Performance (No Constraints)

| Length | Solutions | Expressions Searched | Time | Speed (expr/s) |
|--------|-----------|---------------------|------|----------------|
| 3 | 54 | 99 | ~15µs | ~6.6M |
| 4 | 308 | 582 | ~80µs | ~7.3M |
| 5 | 6,243 | 13,136 | ~1.3ms | ~10M |
| 6 | 49,952 | 108,487 | ~10ms | ~11M |
| 7 | 648,955 | 1,535,857 | ~125ms | ~12M |

## Expression Evaluation

| Expression | Result | Notes |
|-----------|--------|-------|
| `1+2` | 3 | Basic addition |
| `2^10` | 1024 | Power |
| `5!` | 120 | Factorial |
| `5A3` | 60 | Permutation (5*4*3) |
| `[7/2]` | 3 | Floor division |

## Equation Validation

| Equation | Valid | Reason |
|----------|-------|--------|
| `1+2=3` | ✅ | Simple valid equation |
| `2*3=6` | ✅ | Multiplication |
| `5!=120` | ✅ | Factorial |
| `5A3=60` | ✅ | Permutation |
| `[7/2]*2=6` | ✅ | Floor brackets |
| `3-5=-2` | ✅ | Negative RHS |
| `5>=3` | ✅ | Greater-or-equal |
| `6=2*3` | ❌ | RHS must be simple number |
| `1+2` | ❌ | No main operator |
| `5>5` | ❌ | 5 is not > 5 |

## Bounded-Memory Modes (RHS value index)

Top-N and streaming are the memory-bounded modes intended for long puzzles.
The `>` operator's right-hand side is enumerated once into a value-sorted
table and reused by every left-hand-side prefix, rather than being re-searched
for each one. Criterion, 2 threads, `--top 10`:

| Mode | Length | Recursive | Indexed | Speedup |
|------|--------|----------:|--------:|--------:|
| Top-N | 6 | 12.7 ms | 4.2 ms | 3.0× |
| Top-N | 7 | 171 ms | 50 ms | 3.4× |
| Top-N | 8 | 1.71 s | 390 ms | 4.4× |
| Streaming | 6 | 5.9 ms | 2.4 ms | 2.5× |
| Streaming | 7 | 83 ms | 27 ms | 3.0× |

Index build cost (included in the figures above): 0.6 ms at length 6, 8.2 ms
at length 7, 82 ms at length 8.

Results — solutions, ranking and `searched_count` — are identical to the
unindexed engine; the test suite asserts this against `memory_budget = 0`.

## Peak RSS vs Length

CLI, `--top 5`, 2 threads, default 256 MiB index budget:

| Length | Search space | Time | Peak RSS |
|--------|--------------|-----:|---------:|
| 7 | 1.5M | 0.04s | 37 MB |
| 8 | 14.3M | 0.33s | 47 MB |
| 9 | 170M | 3.9s | 136 MB |
| 10 | 1.7B | 91s | 228 MB |

Memory plateaus instead of tracking the length — at the default budget a
length-30 solve peaks at the same ~228 MB as a length-20 one, and at
`--memory-budget-mb 16` both sit near 50 MB.

## Approximate Top-N at Extreme Lengths

Past ~length 10 the space cannot be enumerated exhaustively at any memory
budget. With `--timeout-secs 5` (2 threads) the solver returns the best
ranking found within the budget, flagged approximate:

| Length | Result | Peak RSS |
|--------|--------|---------:|
| 12 | `10+2*3>4-5^6` | 228 MB |
| 20 | `5^10+10+10+2%3*4-6>7` | 228 MB |
| 30 | `5^10+10+10+10+10+10+12%3*4-6>7` | 228 MB |

Each was independently confirmed with `sumzle-solver validate`.
