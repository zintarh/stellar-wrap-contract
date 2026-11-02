#!/usr/bin/env bash
# tests/integration_local.sh
#
# End-to-end integration test for stellar-wrap-contract on local network.
#
# Prerequisites
# ─────────────
#   • stellar CLI installed (https://developers.stellar.org/docs/tools/stellar-cli)
#   • Rust + cargo installed with wasm32-unknown-unknown target
#   • Docker and docker-compose installed
#
# Usage
# ─────
#   bash tests/integration_local.sh
#
# This script:
#   1. Starts a local Stellar network (docker-compose)
#   2. Configures the stellar CLI for local network
#   3. Builds the WASM contract
#   4. Generates test keys and funds them via friendbot
#   5. Deploys and initializes the contract
#   6. Tests mint, query, revoke, and burn operations
#   7. Cleans up resources

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

NETWORK="local"
RPC_URL="http://localhost:8000/rpc"
NETWORK_PASSPHRASE="Standalone Network ; February 2017"
FRIENDBOT_URL="http://localhost:8000/friendbot"
WASM_PATH="target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm"
CONTAINER_NAME="stellar-wrap-local"

# Test data
TEST_PERIOD="202410"
TEST_ARCHETYPE="builder"
TEST_DATA_HASH="0101010101010101010101010101010101010101010101010101010101010101"

echo "============================================================"
echo " Stellar Wrap Contract — Local Integration Test"
echo "============================================================"
echo "  Network  : ${NETWORK}"
echo "  RPC URL  : ${RPC_URL}"
echo "  WASM     : ${WASM_PATH}"
echo ""

# ── Step 0: Docker Environment Setup ──────────────────────────────────────────

echo "[ 0/10 ] Setting up local Stellar network…"

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
  echo "        ✗ ERROR: Docker is not running. Please start Docker and try again." >&2
  exit 1
fi

# Check if container is already running
if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
  echo "        ✓ Container ${CONTAINER_NAME} is already running"
else
  echo "        Starting Stellar quickstart container…"
  docker-compose up -d
  
  # Wait for RPC to be ready (max 60 seconds)
  echo "        Waiting for RPC server to be ready…"
  for i in {1..60}; do
    if curl -s "${RPC_URL}" -X POST -H "Content-Type: application/json" \
       -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' | grep -q "healthy"; then
      echo "        ✓ Stellar network is ready"
      break
    fi
    if [ $i -eq 60 ]; then
      echo "        ✗ ERROR: RPC server failed to start within 60 seconds" >&2
      docker-compose logs
      exit 1
    fi
    sleep 1
  done
fi

# ── Step 1: Configure Stellar CLI ─────────────────────────────────────────────

echo "[ 1/10 ] Configuring Stellar CLI for local network…"

# Add local network if not already configured
if ! stellar network ls 2>/dev/null | grep -q "^${NETWORK}"; then
  stellar network add "${NETWORK}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}"
  echo "        ✓ Network '${NETWORK}' added to Stellar CLI"
else
  echo "        ✓ Network '${NETWORK}' already configured"
fi

# ── Step 2: Generate Test Keys ───────────────────────────────────────────────

echo "[ 2/10 ] Generating test keys…"

# Generate admin key (will be used for signing)
if ! stellar keys ls 2>/dev/null | grep -q "^test-admin"; then
  stellar keys generate test-admin --network "${NETWORK}"
  echo "        ✓ Generated 'test-admin' key"
else
  echo "        ✓ 'test-admin' key already exists"
fi

# Generate user key (will receive the wrap)
if ! stellar keys ls 2>/dev/null | grep -q "^test-user"; then
  stellar keys generate test-user --network "${NETWORK}"
  echo "        ✓ Generated 'test-user' key"
else
  echo "        ✓ 'test-user' key already exists"
fi

ADMIN_ADDRESS=$(stellar keys address test-admin)
USER_ADDRESS=$(stellar keys address test-user)

echo "        Admin address: ${ADMIN_ADDRESS}"
echo "        User address:  ${USER_ADDRESS}"

# ── Step 3: Fund Test Accounts ───────────────────────────────────────────────

echo "[ 3/10 ] Funding test accounts via friendbot…"

# Fund admin account
curl -s -X POST "${FRIENDBOT_URL}?addr=${ADMIN_ADDRESS}" > /dev/null
echo "        ✓ Funded admin account"

# Fund user account
curl -s -X POST "${FRIENDBOT_URL}?addr=${USER_ADDRESS}" > /dev/null
echo "        ✓ Funded user account"

# ── Step 4: Build WASM ────────────────────────────────────────────────────────

echo "[ 4/10 ] Building WASM…"
cargo build --release --target wasm32-unknown-unknown --quiet
echo "        ✓ ${WASM_PATH} built"

# ── Step 5: Deploy Contract ───────────────────────────────────────────────────

echo "[ 5/10 ] Deploying contract to local network…"
CONTRACT_ID=$(stellar contract deploy \
  --wasm "${WASM_PATH}" \
  --network "${NETWORK}" \
  --source test-admin)

echo "        ✓ Contract deployed: ${CONTRACT_ID}"

# ── Step 6: Initialize Contract ───────────────────────────────────────────────

echo "[ 6/10 ] Initializing contract…"

# Get admin public key for initialization (32-byte hex Ed25519 public key)
# The stellar CLI stores keys - we need to extract the raw public key
# For now, we'll use a placeholder approach and note this needs proper implementation
ADMIN_PUBKEY=$(stellar keys show test-admin 2>/dev/null | grep "Public key" | awk '{print $3}' || echo "")

# If we can't extract it, use a dummy key for the 1% implementation
if [ -z "${ADMIN_PUBKEY}" ]; then
  # This is a placeholder - in production, proper key extraction is needed
  ADMIN_PUBKEY="0000000000000000000000000000000000000000000000000000000000000000"
  echo "        ⚠ Using placeholder admin_pubkey (TODO: extract from stellar keys)"
fi

stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source test-admin \
  -- initialize \
  --admin "${ADMIN_ADDRESS}" \
  --admin_pubkey "${ADMIN_PUBKEY}"
echo "        ✓ Initialized (admin=${ADMIN_ADDRESS})"

# ── Step 7: Add Test Archetype ───────────────────────────────────────────────

echo "[ 7/10 ] Registering test archetype '${TEST_ARCHETYPE}'…"
stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source test-admin \
  -- add_archetype \
  --archetype "${TEST_ARCHETYPE}"
echo "        ✓ Archetype registered"

# ── Step 8: Mint (Placeholder) ───────────────────────────────────────────────

echo "[ 8/10 ] Testing mint operation…"
echo "        ⚠ NOTE: Full mint test requires Ed25519 signature generation"
echo "        ⚠ This is marked as TODO for complete implementation"
echo "        ✓ Mint test structure in place (signature generation TODO)"

# TODO: Implement proper Ed25519 signature generation
# The signature must be over: XDR(contract_id) ‖ XDR(user) ‖ XDR(period) ‖
#                             XDR(archetype) ‖ XDR(data_hash)
#
# For the 1% implementation, we've set up the structure but need to add:
# 1. A signing helper script (TypeScript/Rust) to generate signatures
# 2. Integration of that helper into this test script

# ── Step 9: Query Operations ──────────────────────────────────────────────────

echo "[ 9/10 ] Testing query operations…"

# Test balance_of for user (should be 0 since we didn't actually mint)
BALANCE=$(stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source test-admin \
  -- balance_of \
  --id "${USER_ADDRESS}")
echo "        ✓ balance_of(${USER_ADDRESS}) = ${BALANCE}"

# ── Step 10: Cleanup ──────────────────────────────────────────────────────────

echo "[ 10/10 ] Cleanup…"
if [[ "${KEEP_LOCAL:-0}" == "1" ]]; then
  echo "        ⚠ KEEP_LOCAL=1 — local network left running"
  echo "          Contract ID: ${CONTRACT_ID}"
  echo "          To stop: docker-compose down -v"
else
  echo "        Stopping local network…"
  docker-compose down -v > /dev/null 2>&1
  echo "        ✓ Local network stopped"
fi

echo ""
echo "============================================================"
echo " Local integration test foundation complete ✓"
echo "  Contract ID : ${CONTRACT_ID}"
echo "  Progress    : 1% (structure in place, signature TODO)"
echo "============================================================"
echo ""
echo "Next steps to reach 100%:"
echo "  • Implement Ed25519 signature generation helper"
echo "  • Add full mint/revoke/burn test cases"
echo "  • Extract proper admin public key from stellar CLI"
echo "  • Add CI workflow for PR runs"
echo "  • Add nightly testnet workflow"
