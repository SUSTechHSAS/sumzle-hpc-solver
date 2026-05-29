# ===== Stage 1: Build the Rust backend =====
FROM rust:1.82-bookworm AS backend-builder

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Copy actual source code
COPY src/ src/
RUN touch src/main.rs && cargo build --release

# ===== Stage 2: Build the frontend =====
FROM node:20-bookworm AS frontend-builder

WORKDIR /app/frontend

COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build

# ===== Stage 3: Production image =====
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy backend binary
COPY --from=backend-builder /app/target/release/sumzle-solver /app/sumzle-solver

# Copy frontend dist
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist

# Expose port
EXPOSE 3000

# Set environment
ENV RUST_LOG=info

# Run the server
CMD ["/app/sumzle-solver", "serve", "--host", "0.0.0.0", "--port", "3000"]
