#!/usr/bin/env bash
# scripts/check_wasm_size.sh
# Measures compiled Soroban contract WASM binary size against the budget.
# See SIGNATURE_VERIFICATION_DECISION.md for architectural and trade-off details.

set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f ".github/wasm-size-limit" ]; then
    BUDGET_BYTES=$(tr -d '[:space:]' < .github/wasm-size-limit)
else
    BUDGET_BYTES=204800 # 200 KB default
fi

# Determine optimal target for active rustc toolchain
if rustc --print target-list 2>/dev/null | grep -q "^wasm32v1-none$"; then
    TARGET="wasm32v1-none"
else
    TARGET="wasm32-unknown-unknown"
fi

echo "==> Building release WASM with target: ${TARGET}..."
cargo build --target "${TARGET}" --release

# Locate the compiled wasm file
WASM_FILE="target/${TARGET}/release/stellar_wrap_contract.wasm"

if [ ! -f "${WASM_FILE}" ]; then
    # Fallback search across target directory
    WASM_FILE=$(find target -name "stellar_wrap_contract.wasm" | head -n 1 || true)
fi

if [ -z "${WASM_FILE}" ] || [ ! -f "${WASM_FILE}" ]; then
    echo "Error: Could not locate compiled WASM binary." >&2
    exit 1
fi

SIZE_BYTES=$(wc -c < "${WASM_FILE}" | tr -d ' ')
SIZE_KB=$(awk "BEGIN {printf \"%.2f\", ${SIZE_BYTES} / 1024}")
BUDGET_KB=$(awk "BEGIN {printf \"%.2f\", ${BUDGET_BYTES} / 1024}")
BUDGET_PERCENT=$(awk "BEGIN {printf \"%.2f\", (${SIZE_BYTES} / ${BUDGET_BYTES}) * 100}")

echo ""
echo "========================================================"
echo "          SOROBAN WASM SIZE BUDGET REPORT              "
echo "========================================================"
echo "WASM Artifact  : ${WASM_FILE}"
echo "Compile Target : ${TARGET}"
echo "Artifact Size  : ${SIZE_BYTES} bytes (${SIZE_KB} KB)"
echo "WASM Budget    : ${BUDGET_BYTES} bytes (${BUDGET_KB} KB)"
echo "Budget Usage   : ${BUDGET_PERCENT}%"

if [ "${SIZE_BYTES}" -le "${BUDGET_BYTES}" ]; then
    HEADROOM=$((BUDGET_BYTES - SIZE_BYTES))
    HEADROOM_KB=$(awk "BEGIN {printf \"%.2f\", ${HEADROOM} / 1024}")
    echo "Budget Status  : PASSED (${HEADROOM_KB} KB headroom remaining)"
    echo "========================================================"
else
    OVER=$((SIZE_BYTES - BUDGET_BYTES))
    OVER_KB=$(awk "BEGIN {printf \"%.2f\", ${OVER} / 1024}")
    echo "Budget Status  : OVER BUDGET (+${OVER_KB} KB / +${OVER} bytes)"
    echo "Note           : In-guest ed25519-dalek adds ~44.44 KB (19.12% overhead)."
    echo "                 See SIGNATURE_VERIFICATION_DECISION.md for details."
    echo "========================================================"
fi
