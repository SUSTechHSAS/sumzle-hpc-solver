# Sumzle HPC Solver

A high-performance solver for the [Sumzle](https://sustechhsas.github.io/Sumzle/SumzleAK.html) equation puzzle, implemented in Rust with multi-core and distributed computing support, plus a modern web frontend.

## About Sumzle

Sumzle is a Wordle-like game for mathematical equations. Players guess equations of a given length using digits (0-9), operators (+, -, ×, ÷, %, ^), brackets, factorial (!), permutation (A), and comparison operators (=, >=). After each guess, tiles are colored green (correct position), yellow (present but wrong position), or gray (absent), just like Wordle.

## Features

- **Single-threaded solver** with brute-force search and extensive pruning
- **Multi-core parallelism** using [Rayon](https://github.com/rayon-rs/rayon)
- **Distributed computing** via TCP coordinator/worker architecture
- **Web API server** using [axum](https://github.com/tokio-rs/axum) with REST endpoints
- **Modern web frontend** built with React + TypeScript + Vite
- **Behavioral consistency** with the reference JavaScript implementation
- **Cross-platform** builds for Linux, macOS, and Windows
- **Comprehensive test suite with 58 tests (37 Rust core + 7 API integration + 29 frontend)
- **Benchmark suite** using Criterion
- **Docker support** with multi-stage builds

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

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Node.js 18+ and npm (for frontend development)

### Build Backend

```bash
# Debug build
cargo build

# Release build (recommended for benchmarks)
cargo build --release
```

### Build Frontend

```bash
cd frontend
npm install
npm run build
```

### Build with Docker

```bash
docker build -t sumzle-solver .
```

## Usage

### Web Interface

Start the web API server:

```bash
# Start backend server
./sumzle-solver serve --host 0.0.0.0 --port 3000

# In another terminal, start the frontend dev server
cd frontend && npm run dev
```

Then open http://localhost:5173 in your browser.

For production, build the frontend and serve static files through the Rust server or a reverse proxy.

### Solve a puzzle (CLI)

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

### Web API

The web server provides REST API endpoints:

#### Solve a puzzle

```bash
curl -X POST http://localhost:3000/api/solve?threads=0 \
  -H "Content-Type: application/json" \
  -d '{
    "length": 5,
    "rows": [
      {
        "tiles": [
          {"char": "1", "state": "correct"},
          {"char": "+", "state": "present"},
          {"char": "2", "state": "empty"},
          {"char": "=", "state": "correct"},
          {"char": "3", "state": "empty"}
        ]
      }
    ]
  }'
```

Response:
```json
{
  "solutions": ["1+2=3", "1+3=4", ...],
  "stats": {
    "searched_count": 86281,
    "found_count": 6049,
    "elapsed_ms": 37,
    "speed": 2329216
  }
}
```

#### Validate an equation

```bash
curl -X POST http://localhost:3000/api/validate \
  -H "Content-Type: application/json" \
  -d '{"equation": "1+2=3"}'
```

Response: `{"valid": true}`

#### Evaluate an expression

```bash
curl -X POST http://localhost:3000/api/eval \
  -H "Content-Type: application/json" \
  -d '{"expression": "5!"}'
```

Response: `{"result": "120"}`

### Validate an equation (CLI)

```bash
./sumzle-solver validate "1+2=3"     # true
./sumzle-solver validate "6=2*3"     # false (RHS must be simple number)
```

### Evaluate an expression (CLI)

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

### Quick Commands (Makefile)

```bash
make build          # Build backend in debug mode
make release        # Build backend in release mode
make test           # Run all tests (backend + frontend)
make test-backend   # Run Rust tests only
make test-frontend  # Run frontend tests only
make lint           # Lint all code
make serve          # Start the web API server
make dev            # Start both backend and frontend for development
make docker-build   # Build Docker image
make clean          # Clean build artifacts
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

### Web Frontend

The frontend is a React + TypeScript application built with Vite:
- **Puzzle Input**: Interactive Wordle-style tile interface with color-coded states
- **Solve**: Submits puzzles to the backend API and displays results
- **Results**: Shows solutions with statistics (count, search time, speed)
- **Tools**: Expression evaluator and equation validator utilities

## Testing

```bash
# Run all Rust tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run frontend tests
cd frontend && npm test

# Run all tests via Makefile
make test
```

## Benchmarking

```bash
# Run Criterion benchmarks
cargo bench
```

## Project Structure

```
sumzle-hpc-solver/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library exports
│   ├── server.rs        # Web API server (axum)
│   ├── solver.rs        # Brute-force solver with pruning
│   ├── evaluator.rs     # Expression evaluator
│   ├── constraints.rs   # Constraint preprocessing
│   ├── parallel.rs      # Multi-core parallel solver
│   ├── distributed.rs   # Distributed computing
│   └── types.rs         # Core types
├── frontend/
│   ├── src/
│   │   ├── App.tsx           # Main app component
│   │   ├── api.ts            # API client
│   │   ├── types.ts          # TypeScript types
│   │   └── components/       # React components
│   │       ├── Tile.tsx       # Interactive puzzle tile
│   │       ├── GuessRow.tsx   # Row of tiles
│   │       ├── Results.tsx    # Solver results display
│   │       ├── ExpressionEvaluator.tsx
│   │       └── EquationValidator.tsx
│   ├── vite.config.ts
│   └── package.json
├── benches/
│   └── benchmark.rs
├── .github/workflows/
│   ├── ci.yml              # Full CI pipeline
│   └── pr-validation.yml   # PR checks
├── Cargo.toml
├── Makefile
├── Dockerfile
└── README.md
```

## License

MIT
