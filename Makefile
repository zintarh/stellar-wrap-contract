.PHONY: build test fmt fmt-check lint clean deploy-testnet wasm-build docker-build docker-build-verify

# ── Build ────────────────────────────────────────────────────────────────────

## build: Compile the contract to WASM (release profile, wasm32 target)
build: wasm-build

## wasm-build: Explicit WASM release build (output: target/wasm32-unknown-unknown/release/*.wasm)
wasm-build:
	cargo build --release --target wasm32-unknown-unknown

## soroban-build: Build via the Stellar CLI (alternative to cargo build --target wasm32)
soroban-build:
	stellar contract build

# ── Test ─────────────────────────────────────────────────────────────────────

## test: Run all unit and integration tests
test:
	cargo test

## test-verbose: Run all tests with stdout output (useful for gas analysis)
test-verbose:
	cargo test -- --nocapture --test-threads=1

# ── Format ───────────────────────────────────────────────────────────────────

## fmt: Auto-format source code with rustfmt
fmt:
	cargo fmt

## fmt-check: Check formatting without modifying files (CI-safe)
fmt-check:
	cargo fmt --check

# ── Lint ─────────────────────────────────────────────────────────────────────

## lint: Run clippy and treat all warnings as errors
lint:
	cargo clippy -- -D warnings

# ── Deploy ───────────────────────────────────────────────────────────────────

## deploy-testnet: Build and deploy the contract to Stellar testnet.
##   Requires: stellar CLI, STELLAR_DEPLOYER_SECRET env var set.
##   Usage: make deploy-testnet
##   Optional: CONTRACT_ID=<id> to upgrade an existing contract instead of deploying fresh.
deploy-testnet: wasm-build
	@if [ -n "$(CONTRACT_ID)" ]; then \
		echo "Upgrading existing contract $(CONTRACT_ID)…"; \
		stellar contract upload \
			--wasm target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm \
			--network testnet \
			--source "$(STELLAR_DEPLOYER_SECRET)"; \
	else \
		echo "Deploying new contract to testnet…"; \
		stellar contract deploy \
			--wasm target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm \
			--network testnet \
			--source "$(STELLAR_DEPLOYER_SECRET)"; \
	fi

# ── Clean ────────────────────────────────────────────────────────────────────

## clean: Remove build artifacts
clean:
	cargo clean

# ── Docker ───────────────────────────────────────────────────────────────────

## docker-build: Build the contract inside Docker for reproducible WASM output
docker-build:
	docker build -t stellar-wrap-contract .

## docker-build-verify: Build in Docker and print SHA-256 of the WASM artifact
docker-build-verify:
	docker build -t stellar-wrap-contract-verify .
	docker run --rm stellar-wrap-contract-verify sha256sum /contract/target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm

# ── Help ─────────────────────────────────────────────────────────────────────

## help: List all available make targets with descriptions
help:
	@grep -E '^## ' Makefile | sed 's/## /  /'
