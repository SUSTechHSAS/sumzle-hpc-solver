.PHONY: all build release test test-backend test-frontend lint lint-backend lint-frontend clean serve dev docker-build docker-run help

# Default target
all: build

help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

# ===== Rust Backend =====

build: ## Build the project in debug mode
	cargo build

release: ## Build the project in release mode
	cargo build --release

test: test-backend test-frontend ## Run all tests (backend + frontend)

test-backend: ## Run Rust backend tests
	cargo test --verbose

lint-backend: ## Lint Rust code (fmt + clippy)
	cargo fmt -- --check
	cargo clippy -- -D warnings

# ===== Frontend =====

frontend-install: ## Install frontend dependencies
	cd frontend && npm ci

test-frontend: ## Run frontend tests
	cd frontend && npm test

lint-frontend: ## Lint frontend code
	cd frontend && npm run lint

frontend-build: ## Build frontend for production
	cd frontend && npm run build

frontend-dev: ## Start frontend dev server
	cd frontend && npm run dev

# ===== Combined =====

lint: lint-backend lint-frontend ## Lint all code

serve: ## Start the web API server
	cargo run -- serve --host 0.0.0.0 --port 3000

dev: ## Start both backend and frontend for development
	@echo "Starting backend server on :3000 and frontend dev server on :5173"
	@echo "Frontend will proxy /api requests to the backend"
	@$(MAKE) -j2 serve frontend-dev

# ===== Docker =====

docker-build: ## Build Docker image
	docker build -t sumzle-solver .

docker-run: ## Run Docker container
	docker run -p 3000:3000 sumzle-solver

# ===== Cleanup =====

clean: ## Clean build artifacts
	cargo clean
	cd frontend && rm -rf dist node_modules
