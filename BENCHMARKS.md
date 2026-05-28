# Benchmark Results

All benchmarks run single-threaded in release mode.

## Test Environment
- **Date**: 2026-05-28
- **Cores**: 4
- **Build**: `cargo build --release`
- **Rust**: 1.95.0

## Solver Performance (No Constraints)

| Length | Solutions | Expressions Searched | Time | Speed (expr/s) |
|--------|-----------|---------------------|------|----------------|
| 3 | 54 | 841 | 275µs | ~3.1M |
| 4 | 236 | 5,143 | 2.2ms | ~2.3M |
| 5 | 6,049 | 86,281 | 37.5ms | ~2.3M |
| 6 | 37,730 | 685,561 | 367ms | ~1.9M |

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
