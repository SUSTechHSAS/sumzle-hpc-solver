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
- **RHS value index** — the `>` operator's right-hand side is enumerated once into a value-sorted table and reused by every left-hand-side prefix, instead of being re-searched for each one (3–5× faster; memory capped by `--memory-budget-mb`)
- **Approximate top-N for extreme lengths** — `--timeout-secs` / `--max-searched` return the best ranking found within a budget, so lengths far past what an exhaustive search could enumerate still answer
- **Streaming output** — stream solutions as NDJSON to a file without ever holding the full set in memory (CLI `--output`, API `POST /api/solve/stream`)
- **Real-time progress** — a Server-Sent Events endpoint (`POST /api/solve/progress`) reports live multi-thread search progress to drive a real progress bar
- **Steady server memory** — the [mimalloc](https://github.com/microsoft/mimalloc) global allocator returns freed memory to the OS, so the server's resident set stays flat across many solves
- **Behavioral consistency** with the reference JavaScript implementation
- **Cross-platform** builds for Linux, macOS, and Windows
- **Comprehensive test suite** — 205 automated tests (145 Rust: unit, API, and consistency; 60 frontend)
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

### Bounded-memory modes at large lengths

The memory-bounded modes (top-N and streaming) are the ones intended for long
puzzles. Measured on 2 cores, `--top 5`, default 256 MiB index budget:

| Length | Search space | Top-N time | Peak RSS |
|--------|--------------|-----------:|---------:|
| 7 | 1.5M | 0.04s | 37 MB |
| 8 | 14.3M | 0.33s | 47 MB |
| 9 | 170M | 3.9s | 136 MB |
| 10 | 1.7B | 91s | 228 MB |

Peak RSS **plateaus** rather than growing with the length: past the point where
the index budget binds, a length-30 solve uses the same memory as a length-20
one (~228 MB at the default budget, ~50 MB at `--memory-budget-mb 16`).

Beyond ~length 10 the search space is too large to enumerate exhaustively in
any amount of memory, so top-N takes a time budget and returns the best ranking
it found:

```bash
# Length 30: answers in 5s, flat ~228 MB, result marked approximate
./sumzle-solver solve -i puzzle.json --top 3 --timeout-secs 5
```

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

## Android App (Tauri)

The solver can be packaged as a **standalone Android app** (Tauri 2) that runs the
Rust solver **on-device** — no server, works fully offline. The React UI calls the
solver over Tauri IPC (`invoke` + a `Channel` for live progress); the same frontend
still runs on the web unchanged (`frontend/src/api.ts` branches on `isTauri()`).

The mobile crate (`src-tauri/`) depends on the core with `default-features = false`,
so the web stack (axum/tokio/tower) is **not** compiled into the APK — only the
solver core (`src/api.rs`: `solve_core` / `validate` / `eval`).

### Build workflow

Android builds are heavy (Rust + NDK + Gradle), so they run on a **remote build host**
while a local **waydroid** instance is used for debugging. Both are scripted:

```bash
# 1. One-time: install the toolchain on the remote (rustup + android targets,
#    JDK, Android SDK/NDK, cargo-tauri). The host is passed each time.
REMOTE=user@host ./scripts/remote-bootstrap.sh

# 2. Build the APK on the remote and fetch it to ./build/
#    abi: x86_64 (waydroid, default) | aarch64 (phones) | armv7 | i686
REMOTE=user@host ./scripts/remote-build.sh x86_64

# 3. Install + launch in local waydroid (container service must be running)
./scripts/waydroid-run.sh
```

To build directly (toolchain already present locally), from the repo root:

```bash
cargo tauri android init                                   # once, scaffolds src-tauri/gen/android
cargo tauri android build --debug --apk --target x86_64    # → app-universal-debug.apk
```

### Continuous integration

`.github/workflows/android.yml` builds both ABIs (`x86_64` for emulators/waydroid,
`aarch64` for phones) as debug APKs and uploads them as workflow artifacts — on
pushes to `main`, tags, manual dispatch, and PRs touching `src-tauri/`, `src/`,
`frontend/`, or the Cargo manifests. The main `ci.yml` (fmt/clippy/test) is
unaffected: the workspace's default member is the core crate, so its bare `cargo`
commands don't build `src-tauri`.

### Notes & gotchas

- **Use the `cargo-tauri` CLI** (`cargo install tauri-cli`), *not* the npm-global
  `tauri` — the npm CLI makes Gradle's rust task call `node tauri …` which fails to
  resolve. If you switched, `rm -rf src-tauri/gen/android && cargo tauri android init`.
- **waydroid on a PC is x86_64**, so build the `x86_64` ABI for it; real phones need
  `aarch64`.
- The debug `[profile.dev]` strips DWARF debuginfo (`strip = "debuginfo"`) to keep the
  Android `.so` ~20 MB instead of ~120 MB; backtraces keep symbol names.
- **Release / Play Store**: `RELEASE=1 ./scripts/remote-build.sh aarch64` builds a
  release APK, but release builds must be signed with your own keystore (configure
  `src-tauri/gen/android` signing) — debug builds are auto-signed and fine for waydroid.
- File export on mobile (`流式保存到文件`) is not yet wired to the Tauri fs/dialog
  plugins; it is disabled in the app for now (the core solve/eval/validate all work).

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

# Cap the RHS value index (default 256 MiB). This is a hard ceiling on the
# solver's length-dependent memory: whatever does not fit falls back to the
# plain recursive search, with identical results. 0 disables indexing.
./sumzle-solver solve -i puzzle.json --top 10 --memory-budget-mb 64

# Extremely long puzzles: stop after a budget and return the best ranking
# found so far. Prints a warning that the result is APPROXIMATE.
./sumzle-solver solve -i puzzle.json --top 3 --timeout-secs 30
./sumzle-solver solve -i puzzle.json --top 3 --max-searched 500000000
```

#### Exactness

Without `--timeout-secs` / `--max-searched` the solver is **exhaustive**: the
solutions, their ranking and `searched_count` are identical whether or not the
index is used, at any thread count (this is asserted in the test suite against
the unindexed engine). With a budget the search may stop early, and the CLI
says so on stderr — the ranking and character statistics are then drawn from
the part of the space that was explored, and a wall-clock budget is not
reproducible run to run.

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

CI benchmark runs publish a structured `benchmark-result.json` artifact. The
dedicated benchmark dashboard workflow merges those artifacts into GitHub Pages
history so `main` and PR performance can be inspected as interactive curves.
The dashboard's filters are toggle chips: pick several branches, suites,
metrics, or parameter sets to overlay them in one chart, and click a legend
item to show or hide its view. Selections persist in the URL, so shared links
restore the exact overlay. Changing one filter never resets the others.

The dashboard has a Playwright end-to-end suite covering filter preservation,
multi-view overlay, and persistence:

```bash
node scripts/ci/bench/dashboard-tests/spec.js
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

### RHS Value Index

The `=` operator resolves its right-hand side in O(1) — once the left-hand side
evaluates to `v`, the only possible RHS is the decimal spelling of `v`. The `>`
operator had no such shortcut and re-enumerated the whole RHS subtree for every
LHS prefix, which is where essentially all the time went at large lengths.

Since that subtree does not depend on the prefix, each RHS length is now
enumerated **once** into a table sorted by value. A prefix then resolves its
entire RHS subtree with a single binary search: `lhs > rhs` holds for exactly a
leading run of the table. Consumers take that range in bulk —

- **counting** (top-N pass 1) reads cumulative block prefix sums,
- **ranking** (top-N pass 2) prunes with an OR-tree, where a node's OR of
  character masks bounds the best score achievable beneath it,
- **streaming** expands the range entry by entry, since it must emit each line.

The table is capped by `--memory-budget-mb`, and the cap covers the build-time
peak, not just the finished table. Count constraints (from `present` tiles)
couple the two sides of the expression, so they disable the index; positional
constraints are safe and are compiled into it.

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

The repository also has a dedicated GitHub Actions benchmark pipeline:

- `.github/workflows/benchmark.yml` runs CLI, parallel, Top-N, streaming,
  memory, server, and Criterion benchmarks.
- `.github/workflows/benchmark-pages.yml` publishes a static dashboard backed by
  `data/history.json`.
- PR runs stay on separate PR curves; only post-merge `main` runs enter the
  main trend.

For first-time setup, configure GitHub Pages to use **GitHub Actions** as its
publishing source.

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
│   ├── rhs_index.rs     # Value-sorted RHS table for the `>` operator
│   ├── limit.rs         # Work budget for approximate (bounded) searches
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
│   ├── ci.yml              # Main CI pipeline (build, lint, tests, release)
│   ├── benchmark.yml       # Benchmark runs and JSON artifact upload
│   ├── benchmark-pages.yml # Benchmark dashboard deployment
│   └── codeql.yml          # CodeQL security analysis
├── Cargo.toml
├── Makefile
├── Dockerfile
└── README.md
```

## License

MIT
