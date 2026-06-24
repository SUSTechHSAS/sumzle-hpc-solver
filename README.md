# Sumzle HPC Solver

A high-performance solver for the [Sumzle](https://sustechhsas.github.io/Sumzle/SumzleAK.html) equation puzzle, implemented in Rust with multi-core and distributed computing support, plus a modern web frontend.

## About Sumzle

Sumzle is a Wordle-like game for mathematical equations. Players guess equations of a given length using digits (0-9), operators (+, -, ×, ÷, %, ^), brackets, factorial (!), permutation (A), and comparison operators (`=`, `>`). After each guess, tiles are colored green (correct position), yellow (present but wrong position), or gray (absent), just like Wordle.

## Features

- **Single-threaded solver** with brute-force search and extensive pruning
- **Multi-core parallelism** using [Rayon](https://github.com/rayon-rs/rayon)
- **Distributed computing** via TCP coordinator/worker architecture
- **Web API server** using [axum](https://github.com/tokio-rs/axum) with REST endpoints
- **Modern web frontend** built with React + TypeScript + Vite
- **Top-N mode** — return only the N highest-scoring solutions with bounded memory (CLI `--top`, API `?top=`)
- **Streaming output** — stream solutions as NDJSON to a file without ever holding the full set in memory (CLI `--output`, API `POST /api/solve/stream`)
- **Real-time progress** — a Server-Sent Events endpoint (`POST /api/solve/progress`) reports live multi-thread search progress to drive a real progress bar
- **Steady server memory** — the [mimalloc](https://github.com/microsoft/mimalloc) global allocator returns freed memory to the OS, so the server's resident set stays flat across many solves
- **Behavioral consistency** with the reference JavaScript implementation
- **Cross-platform** builds for Linux, macOS, and Windows
- **Comprehensive test suite** — 180 automated tests (120 Rust: unit, API, and consistency; 60 frontend)
- **Benchmark suite** using Criterion
- **Docker support** with multi-stage builds

## Performance

Single-threaded, release build on a modern CPU:

| Length | Solutions | Expressions Searched | Time |
|--------|-----------|---------------------|------|
| 3 | 54 | 99 | ~15µs |
| 4 | 308 | 582 | ~80µs |
| 5 | 6,243 | 13,136 | ~1.3ms |
| 6 | 49,952 | 108,487 | ~10ms |
| 7 | 648,955 | 1,535,857 | ~125ms |

Multi-core parallelism provides near-linear speedup with the number of cores.

## Building

### Prerequisites

- Rust 1.82+ (install via [rustup](https://rustup.rs/))
- Node.js 20+ and npm (for frontend development)

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

The server uses the mimalloc allocator so its memory stays steady across many solves. On memory-constrained hosts, set `MIMALLOC_PURGE_DELAY=0` to return freed memory to the OS immediately after each request.

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

# Top-N: keep only the 10 highest-scoring solutions (bounded memory)
./sumzle-solver solve -i puzzle.json --top 10 -f text

# Stream every solution to a file as JSON Lines (never held in memory);
# stdout shows only the statistics
./sumzle-solver solve -i puzzle.json -o solutions.jsonl
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

**Query parameters:**

- `threads` — worker threads (`0` = auto-detect cores, `1` = single-threaded). Default `0`.
- `top` — return only the N highest-scoring solutions (`0` = every solution). Default `0`. Top-N uses a bounded-memory two-pass scorer; `found_count` and the character probabilities still reflect the **full** solution set, only the returned `solutions` list is capped at N.

Response:
```json
{
  "solutions": ["1+2=3", "1+3=4"],
  "stats": {
    "searched_count": 86281,
    "found_count": 6049,
    "elapsed_ms": 37,
    "speed": 2329216
  },
  "char_probabilities": [
    { "char": "=", "display": "=", "count": 6049, "probability": 100.0 }
  ],
  "recommended": "1+2=3",
  "top": 0,
  "scores": []
}
```

When `top` > 0, `solutions` is sorted by score (descending) and `scores` holds the matching per-solution scores; `recommended` is the top-ranked solution.

#### Stream solutions to a file

For very large result sets, stream every solution as [NDJSON](https://ndjson.org/)
(`application/x-ndjson`, one `{"solution":"..."}` per line) instead of buffering
them. Server memory stays bounded regardless of the solution count. The `top`
parameter is ignored — streaming always returns the complete set.

```bash
curl -X POST "http://localhost:3000/api/solve/stream?threads=0" \
  -H "Content-Type: application/json" \
  -d '{"length": 7, "rows": []}' \
  -o solutions.ndjson
```

#### Solve with live progress (Server-Sent Events)

Same request shape as `/api/solve`, but the response is an SSE stream: repeated
`event: progress` frames with `{"done","total","phase"}` (counting completed
search branches across the worker threads), followed by a final `event: result`
frame carrying the full solve response. The web frontend uses this to render a
real progress bar.

```bash
curl -N -X POST "http://localhost:3000/api/solve/progress?threads=0" \
  -H "Content-Type: application/json" \
  -d '{"length": 7, "rows": []}'
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
- **Real-time progress**: Optional live progress bar driven by the SSE endpoint
- **Stream to file**: Save the full solution set straight to disk via the File System Access API (with a download fallback)
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
│   ├── ci.yml              # Full CI pipeline (build, lint, test, benchmarks)
│   └── codeql.yml          # CodeQL security analysis
├── Cargo.toml
├── Makefile
├── Dockerfile
└── README.md
```

## License

MIT
