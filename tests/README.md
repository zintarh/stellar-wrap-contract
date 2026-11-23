# Integration Tests

This directory contains end-to-end integration tests for the stellar-wrap-contract.

## Overview

We have two types of integration tests:

1. **Local Integration Tests** (`integration_local.sh`) - Fast, deterministic tests against a local Stellar network
2. **Testnet Integration Tests** (`integration_testnet.sh`) - Tests against the public Stellar testnet

## Local Integration Tests

### Prerequisites

- Docker and docker-compose installed
- Rust toolchain with `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli) installed

### Usage

```bash
bash tests/integration_local.sh
```

The script will:
1. Start a local Stellar network using docker-compose
2. Build the contract WASM
3. Generate and fund test accounts
4. Deploy and initialize the contract
5. Run integration tests
6. Clean up resources

### Environment Variables

- `KEEP_LOCAL=1` - Keep the local network running after tests (useful for debugging)

### CI Integration

Local tests run automatically on every PR via the `integration-local.yml` workflow.
They complete in under 5 minutes and provide fast feedback.

## Testnet Integration Tests

### Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli) installed
- Funded Stellar testnet account

### Usage

```bash
export STELLAR_DEPLOYER_SECRET=S...
export STELLAR_ADMIN_PUBKEY=aabbcc...
export STELLAR_MINT_SIGNATURE=... # Optional, for mint tests
bash tests/integration_testnet.sh
```

### Environment Variables

- `STELLAR_DEPLOYER_SECRET` (required) - Secret key for testnet account
- `STELLAR_ADMIN_PUBKEY` (required) - 32-byte hex Ed25519 public key
- `STELLAR_MINT_SIGNATURE` (optional) - Valid signature for mint test
- `STELLAR_NETWORK` (optional) - Defaults to "testnet"
- `KEEP_CONTRACT=1` - Keep contract on testnet after tests
- `SKIP_MINT=1` - Skip mint verification step

### CI Integration

Testnet tests run on a nightly schedule via the `integration-testnet-nightly.yml` workflow.
This reduces CI load while still catching issues with real network behavior.

## Implementation Status

### ✅ Completed (1% - Foundation)

- Local test script structure
- Docker Compose configuration for local Stellar network
- Basic contract deployment and initialization flow
- CI workflow for local tests on PRs
- Nightly workflow for testnet tests
- Query operation tests (balance_of, etc.)

### 🚧 TODO (Remaining 99%)

- [ ] Ed25519 signature generation helper
- [ ] Full mint test with valid signature
- [ ] Revoke operation tests
- [ ] Burn operation tests
- [ ] Extract proper admin public key from stellar CLI keys
- [ ] Error case testing
- [ ] Multi-user scenarios
- [ ] Gas cost analysis
- [ ] Performance benchmarks

## Development

### Running Tests Locally

```bash
# Run local integration tests
make test-integration-local

# Run testnet tests (requires credentials)
make test-integration-testnet
```

### Debugging

Keep the local network running after tests:

```bash
KEEP_LOCAL=1 bash tests/integration_local.sh
```

Then inspect the container:

```bash
docker-compose logs
docker exec -it stellar-wrap-local sh
```

View contract state:

```bash
stellar contract invoke --id <CONTRACT_ID> --network local --source test-admin -- balance_of --id <ADDRESS>
```

### Adding New Test Cases

1. Add test logic to `integration_local.sh`
2. Test locally with `KEEP_LOCAL=1`
3. Update this README with test coverage
4. Ensure tests complete in under 5 minutes

## Architecture

```
tests/
├── integration_local.sh          # Fast local tests (PR CI)
├── integration_testnet.sh        # Testnet validation (nightly)
├── storage_fee.rs                # Rust storage tests
└── README.md                     # This file

.github/workflows/
├── integration-local.yml         # PR workflow (local tests)
└── integration-testnet-nightly.yml  # Nightly testnet workflow
```

## Related Issues

- Closes #508 - Docker compose for stellar-core (local network setup)
- Part of effort to move testnet tests to nightly schedule
- Enables deterministic integration testing on every PR
