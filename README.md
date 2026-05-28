# Sumzle HPC Solver

A high-performance solver for the [Sumzle](https://sustechhsas.github.io/Sumzle/SumzleAK.html) equation puzzle, implemented in Rust with multi-core and distributed computing support.

## About Sumzle

Sumzle is a Wordle-like game for mathematical equations. Players guess equations of a given length using digits (0-9), operators (+, -, ×, ÷, %, ^), brackets, factorial (!), permutation (A), and comparison operators (=, >=). After each guess, tiles are colored green (correct position), yellow (present but wrong position), or gray (absent), just like Wordle.

## Features

- **Single-threaded solver** with brute-force search and extensive pruning
- **Multi-core parallelism** using [Rayon](https://github.com/rayon-rs/rayon)
- **Distributed computing** via TCP coordinator/worker architecture
- **Behavioral consistency** with the reference JavaScript implementation
- **Cross-platform** builds for Linux, macOS, and Windows
- **Comprehensive test suite** with 37 tests
- **Benchmark suite** using Criterion

## Performance

Single-threaded, release build on a modern CPU:

| Length | Solutions | Expressions Searched | Time |
|--------|-----------|---------------------|------|
| 3 | 54 | 841 | 275µs |
| 4 | 236 | 5,143 | 2.2ms |
| 5 | 6,049 | 86,281 | 37.5ms |
| 6 | 37,730 | 685,561 | 367ms |

Multi-core parallelism provides near-linear speedup with the number of cores.

## Building

```bash
# Debug build
cargo build

# Release build (recommended for benchmarks)
cargo build --release
```

## Usage

### Solve a puzzle

Create a JSON input file:

```json
{
  "length": 6,
  "rows": [
    {
      "tiles": [
        {"char": "1", "state": "correct"},
        {"char": "+", "state": "present"},
        {"char": "2", "state": "empty"},
        {"char": "=", "state": "correct"},
        {"char": "3", "state": "empty"},
        {"char": "0", "state": "empty"}
      ]
    }
  ]
}
```

Run the solver:

```bash
# Single-threaded
./sumzle-solver solve -i puzzle.json -t 1 -f text

# Multi-threaded (auto-detect cores)
./sumzle-solver solve -i puzzle.json -t 0 -f json

# Specific number of threads
./sumzle-solver solve -i puzzle.json -t 4 -f text
```

### Validate an equation

```bash
./sumzle-solver validate "1+2=3"     # true
./sumzle-solver validate "6=2*3"     # false (RHS must be simple number)
```

### Evaluate an expression

```bash
./sumzle-solver eval "5!"           # 120
./sumzle-solver eval "5A3"          # 60
./sumzle-solver eval "[7/2]"        # 3
```

### Run benchmarks

```bash
./sumzle-solver bench -l 5 6
```

### Distributed computing

```bash
# Start coordinator
./sumzle-solver coordinate -p 9876 -i puzzle.json

# Start workers (on other machines)
./sumzle-solver worker -c coordinator-ip:9876 -i worker-1 -t 4
```

## Architecture

### Expression Evaluation

The evaluator supports:
- Basic arithmetic: `+`, `-`, `*`, `/`, `%`, `^`
- Factorial: `n!` (0-12)
- Permutation: `nAr` = n!/(n-r)! (n,r ≤ 10)
- Floor brackets: `[expr]` (evaluates and floors the result)
- Parentheses: `(expr)`

### Constraint Processing

Constraints from Wordle-style feedback are processed into:
- **Fixed characters**: Known correct characters at specific positions
- **Exclusion sets**: Characters that cannot appear at specific positions
- **Minimum counts**: Characters that must appear at least N times
- **Exact counts**: Characters that must appear exactly N times
- **Globally forbidden**: Characters that cannot appear at all

### Search Pruning

The solver uses extensive pruning to avoid exploring invalid branches:
- Character placement constraints
- Floor bracket context tracking
- Expression syntax validation (early rejection)
- Leading zero detection
- Operand value limits (max 30)
- Bracket balance tracking
- Main operator placement rules

## Testing

```bash
# Run all tests
cargo test

# Run with verbose output
cargo test -- --nocapture
```

## Benchmarking

```bash
# Run Criterion benchmarks
cargo bench
```

## License

MIT
