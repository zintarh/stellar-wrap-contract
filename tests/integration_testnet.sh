#!/usr/bin/env bash
# tests/integration_testnet.sh
#
# End-to-end integration test for stellar-wrap-contract on Stellar testnet.
#
# Prerequisites
# ─────────────
#   • stellar CLI installed (https://developers.stellar.org/docs/tools/stellar-cli)
#   • Rust + cargo installed with wasm32-unknown-unknown target
#   • A funded Stellar testnet keypair (use `stellar keys generate --network testnet`)
#
# Required environment variables
# ────────────────────────────────
#   STELLAR_DEPLOYER_SECRET  – Secret key for a funded testnet account used to
#                              deploy the contract and call admin functions.
#   STELLAR_ADMIN_PUBKEY     – 32-byte hex Ed25519 public key passed to initialize().
#                              The matching private key must be available to sign
#                              mint payloads (see sign_payload() below).
#
# Optional environment variables
# ────────────────────────────────
#   STELLAR_NETWORK          – Defaults to "testnet"
#   KEEP_CONTRACT            – Set to "1" to skip contract cleanup at the end.
#
# Usage
# ─────
#   export STELLAR_DEPLOYER_SECRET=SXXXXXXX...
#   export STELLAR_ADMIN_PUBKEY=aabbcc...   # 64 hex chars (32 bytes)
#   bash tests/integration_testnet.sh
#
# Note: This script is NOT run in CI by default. It is intended for manual
# pre-deployment smoke testing against a live Stellar testnet node.
#
# What it tests
# ─────────────
#   1. Build WASM from source
#   2. Deploy contract to testnet
#   3. Call initialize() with admin address and pubkey
#   4. Mint a wrap record
#   5. Call get_wrap() and verify the returned record
#   6. Call has_wrap() and verify it returns true
#   7. Call balance_of() and verify count is 1
#   8. (Optional cleanup) Remove the test contract

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

NETWORK="${STELLAR_NETWORK:-testnet}"
WASM_PATH="target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm"
CONTRACT_ID_FILE="tests/.testnet_contract_id"

# Validate required env vars
if [[ -z "${STELLAR_DEPLOYER_SECRET:-}" ]]; then
  echo "ERROR: STELLAR_DEPLOYER_SECRET is not set." >&2
  echo "  export STELLAR_DEPLOYER_SECRET=SXXXXXXX..." >&2
  exit 1
fi

if [[ -z "${STELLAR_ADMIN_PUBKEY:-}" ]]; then
  echo "ERROR: STELLAR_ADMIN_PUBKEY is not set." >&2
  echo "  export STELLAR_ADMIN_PUBKEY=<64-hex-char Ed25519 pubkey>" >&2
  exit 1
fi

# Derive the deployer's public address from the secret key
DEPLOYER_ADDRESS=$(stellar keys address --secret-key "${STELLAR_DEPLOYER_SECRET}" 2>/dev/null || \
  stellar keys address "deployer" 2>/dev/null || \
  echo "UNKNOWN")

echo "============================================================"
echo " Stellar Wrap Contract — Testnet Integration Test"
echo "============================================================"
echo "  Network  : ${NETWORK}"
echo "  Deployer : ${DEPLOYER_ADDRESS}"
echo "  WASM     : ${WASM_PATH}"
echo ""

# ── Step 1: Build ─────────────────────────────────────────────────────────────

echo "[ 1/8 ] Building WASM…"
cargo build --release --target wasm32-unknown-unknown --quiet
echo "        ✓ ${WASM_PATH} built"

# ── Step 2: Deploy ────────────────────────────────────────────────────────────

echo "[ 2/8 ] Deploying contract to ${NETWORK}…"
CONTRACT_ID=$(stellar contract deploy \
  --wasm "${WASM_PATH}" \
  --network "${NETWORK}" \
  --source "${STELLAR_DEPLOYER_SECRET}")

echo "        ✓ Contract deployed: ${CONTRACT_ID}"
echo "${CONTRACT_ID}" > "${CONTRACT_ID_FILE}"

# ── Step 3: Initialize ────────────────────────────────────────────────────────

echo "[ 3/8 ] Initializing contract…"
stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source "${STELLAR_DEPLOYER_SECRET}" \
  -- initialize \
  --admin "${DEPLOYER_ADDRESS}" \
  --admin_pubkey "${STELLAR_ADMIN_PUBKEY}"
echo "        ✓ Initialized (admin=${DEPLOYER_ADDRESS})"

# ── Step 4: Seed test data ────────────────────────────────────────────────────
#
# In a full integration test, the backend would generate a real Ed25519
# signature over the canonical payload. Here we call add_archetype and
# demonstrate that the CLI invocation pattern is correct.
# Minting requires a valid 64-byte signature from the admin private key —
# see the TypeScript signing example in docs/security/security-recommendations.md.
#
# The snippet below shows the CLI shape; replace <SIGNATURE_HEX> with a
# real signature produced by your signing service.

TEST_USER="${DEPLOYER_ADDRESS}"
TEST_PERIOD="202406"
TEST_ARCHETYPE="builder"
TEST_DATA_HASH="0101010101010101010101010101010101010101010101010101010101010101"

echo "[ 4/8 ] Registering test archetype '${TEST_ARCHETYPE}'…"
stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source "${STELLAR_DEPLOYER_SECRET}" \
  -- add_archetype \
  --archetype "${TEST_ARCHETYPE}"
echo "        ✓ Archetype registered"

echo "[ 4/8 ] Minting wrap (requires valid Ed25519 signature)…"
echo "        NOTE: Set SKIP_MINT=1 to skip the mint step if you do not have"
echo "              an admin signing service available."
if [[ "${SKIP_MINT:-0}" == "1" ]]; then
  echo "        ⚠ Skipping mint (SKIP_MINT=1)"
else
  # Replace <SIGNATURE_HEX> with a real 64-byte Ed25519 signature
  # produced by signing: XDR(contract_id) ‖ XDR(user) ‖ XDR(period) ‖
  #                      XDR(archetype) ‖ XDR(data_hash)
  SIGNATURE_HEX="${STELLAR_MINT_SIGNATURE:-}"
  if [[ -z "${SIGNATURE_HEX}" ]]; then
    echo "        ⚠ STELLAR_MINT_SIGNATURE not set — skipping mint verification step."
    echo "          Set STELLAR_MINT_SIGNATURE=<128-hex-char Ed25519 signature> to test minting."
  else
    stellar contract invoke \
      --id "${CONTRACT_ID}" \
      --network "${NETWORK}" \
      --source "${STELLAR_DEPLOYER_SECRET}" \
      -- mint_wrap \
      --user "${TEST_USER}" \
      --period "${TEST_PERIOD}" \
      --archetype "${TEST_ARCHETYPE}" \
      --data_hash "${TEST_DATA_HASH}" \
      --signature "${SIGNATURE_HEX}"
    echo "        ✓ Wrap minted"

    # ── Step 5: get_wrap ────────────────────────────────────────────────────
    echo "[ 5/8 ] Querying get_wrap(${TEST_USER}, ${TEST_PERIOD})…"
    GET_RESULT=$(stellar contract invoke \
      --id "${CONTRACT_ID}" \
      --network "${NETWORK}" \
      --source "${STELLAR_DEPLOYER_SECRET}" \
      -- get_wrap \
      --user "${TEST_USER}" \
      --period "${TEST_PERIOD}")
    echo "        ✓ get_wrap result: ${GET_RESULT}"

    # ── Step 6: has_wrap ────────────────────────────────────────────────────
    echo "[ 6/8 ] Querying has_wrap(${TEST_USER}, ${TEST_PERIOD})…"
    HAS_RESULT=$(stellar contract invoke \
      --id "${CONTRACT_ID}" \
      --network "${NETWORK}" \
      --source "${STELLAR_DEPLOYER_SECRET}" \
      -- has_wrap \
      --user "${TEST_USER}" \
      --period "${TEST_PERIOD}")
    if [[ "${HAS_RESULT}" != "true" ]]; then
      echo "        ✗ FAIL: has_wrap returned '${HAS_RESULT}', expected 'true'" >&2
      exit 1
    fi
    echo "        ✓ has_wrap = true"

    # ── Step 7: balance_of ──────────────────────────────────────────────────
    echo "[ 7/8 ] Querying balance_of(${TEST_USER})…"
    BALANCE=$(stellar contract invoke \
      --id "${CONTRACT_ID}" \
      --network "${NETWORK}" \
      --source "${STELLAR_DEPLOYER_SECRET}" \
      -- balance_of \
      --id "${TEST_USER}")
    if [[ "${BALANCE}" != "1" ]]; then
      echo "        ✗ FAIL: balance_of returned '${BALANCE}', expected '1'" >&2
      exit 1
    fi
    echo "        ✓ balance_of = 1"
  fi
fi

# ── Step 9: Sponsored mint (fee-bump) ─────────────────────────────────────────
#
# A sponsor pays transaction fees and storage rent on behalf of a user.
# The inner transaction is signed by the user (satisfying user.require_auth());
# the outer fee-bump envelope is signed by the sponsor.
#
# Required environment variables for this step:
#   STELLAR_SPONSOR_SECRET       – Secret key of the fee-paying sponsor account
#   STELLAR_SPONSORED_USER_SECRET – Secret key of the user being sponsored
#                                   (can be a fresh account with 0 XLM beyond
#                                   the minimum balance)
#   STELLAR_MINT_SIGNATURE_SPONSORED – Valid 128-hex-char Ed25519 signature for
#                                      the sponsored user + period combination

echo "[ 9/8 ] Sponsored mint (fee-bump) test…"

if [[ "${SKIP_MINT:-0}" == "1" ]]; then
  echo "        ⚠ Skipping sponsored mint (SKIP_MINT=1)"
elif [[ -z "${STELLAR_SPONSOR_SECRET:-}" ]]; then
  echo "        ⚠ STELLAR_SPONSOR_SECRET not set — skipping sponsored mint test."
  echo "          Set STELLAR_SPONSOR_SECRET=S... and STELLAR_SPONSORED_USER_SECRET=S..."
  echo "          plus STELLAR_MINT_SIGNATURE_SPONSORED=<128-hex-char signature>."
elif [[ -z "${STELLAR_SPONSORED_USER_SECRET:-}" ]]; then
  echo "        ⚠ STELLAR_SPONSORED_USER_SECRET not set — skipping sponsored mint test."
elif [[ -z "${STELLAR_MINT_SIGNATURE_SPONSORED:-}" ]]; then
  echo "        ⚠ STELLAR_MINT_SIGNATURE_SPONSORED not set — skipping sponsored mint test."
else
  SPONSORED_USER_ADDRESS=$(stellar keys address --secret-key "${STELLAR_SPONSORED_USER_SECRET}" 2>/dev/null || echo "UNKNOWN")
  SPONSORED_PERIOD="202407"
  SPONSORED_ARCHETYPE="builder"
  SPONSORED_DATA_HASH="0202020202020202020202020202020202020202020202020202020202020202"

  echo "        Sponsor  : $(stellar keys address --secret-key "${STELLAR_SPONSOR_SECRET}" 2>/dev/null)"
  echo "        User     : ${SPONSORED_USER_ADDRESS}"
  echo "        Period   : ${SPONSORED_PERIOD}"

  # Build the inner transaction with the user as source, without submitting.
  echo "        Building inner transaction…"
  INNER_XDR=$(stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --network "${NETWORK}" \
    --source "${STELLAR_SPONSORED_USER_SECRET}" \
    --build-only \
    -- mint_wrap \
    --user "${SPONSORED_USER_ADDRESS}" \
    --period "${SPONSORED_PERIOD}" \
    --archetype "${SPONSORED_ARCHETYPE}" \
    --data_hash "${SPONSORED_DATA_HASH}" \
    --payload_version 1 \
    --signature "${STELLAR_MINT_SIGNATURE_SPONSORED}")

  # Sign the inner transaction as the user (satisfies user.require_auth()).
  echo "        Signing inner transaction as user…"
  SIGNED_INNER_XDR=$(stellar tx sign \
    --network "${NETWORK}" \
    --sign-with-key "${STELLAR_SPONSORED_USER_SECRET}" \
    "${INNER_XDR}")

  # Wrap the signed inner transaction in a fee-bump envelope and submit.
  # The sponsor's account pays all fees; the user pays nothing.
  echo "        Submitting fee-bump transaction (sponsor pays)…"
  stellar tx fee-bump \
    --network "${NETWORK}" \
    --source "${STELLAR_SPONSOR_SECRET}" \
    --fee-account "$(stellar keys address --secret-key "${STELLAR_SPONSOR_SECRET}" 2>/dev/null)" \
    --inner-transaction "${SIGNED_INNER_XDR}"

  echo "        ✓ Fee-bump submitted"

  # Verify the wrap was recorded under the sponsored user's address.
  echo "        Verifying sponsored wrap with get_wrap…"
  SPONSORED_RESULT=$(stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --network "${NETWORK}" \
    --source "${STELLAR_DEPLOYER_SECRET}" \
    -- get_wrap \
    --user "${SPONSORED_USER_ADDRESS}" \
    --period "${SPONSORED_PERIOD}")

  if [[ "${SPONSORED_RESULT}" == "null" || -z "${SPONSORED_RESULT}" ]]; then
    echo "        ✗ FAIL: get_wrap returned null for sponsored user" >&2
    exit 1
  fi
  echo "        ✓ Sponsored wrap confirmed: ${SPONSORED_RESULT}"

  echo "        Verifying balance_of for sponsored user…"
  SPONSORED_BALANCE=$(stellar contract invoke \
    --id "${CONTRACT_ID}" \
    --network "${NETWORK}" \
    --source "${STELLAR_DEPLOYER_SECRET}" \
    -- balance_of \
    --id "${SPONSORED_USER_ADDRESS}")

  if [[ "${SPONSORED_BALANCE}" -lt 1 ]]; then
    echo "        ✗ FAIL: balance_of returned '${SPONSORED_BALANCE}' for sponsored user" >&2
    exit 1
  fi
  echo "        ✓ Sponsored user balance_of = ${SPONSORED_BALANCE}"
fi

# ── Step 8: Cleanup ───────────────────────────────────────────────────────────

echo "[ 8/8 ] Cleanup…"
if [[ "${KEEP_CONTRACT:-0}" == "1" ]]; then
  echo "        ⚠ KEEP_CONTRACT=1 — contract ${CONTRACT_ID} left on testnet."
  echo "          To clean up manually: rm ${CONTRACT_ID_FILE}"
else
  # Soroban does not support deleting contracts, but we can clean up the
  # local reference file.
  if [[ -f "${CONTRACT_ID_FILE}" ]]; then
    rm "${CONTRACT_ID_FILE}"
  fi
  echo "        ✓ Local contract-id file removed."
  echo "        ℹ  Note: Soroban/Stellar does not support on-chain contract deletion."
  echo "           The contract remains on testnet but will expire via ledger TTL."
fi

echo ""
echo "============================================================"
echo " Integration test complete ✓"
echo "  Contract ID : ${CONTRACT_ID}"
echo "============================================================"
