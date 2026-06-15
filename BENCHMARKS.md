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
