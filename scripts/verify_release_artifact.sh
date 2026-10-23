#!/usr/bin/env bash
set -euo pipefail

TAG="${1:?usage: verify_release_artifact.sh TAG}"
ROOT="$(git rev-parse --show-toplevel)"
TMP_DIR="$(mktemp -d)"
WORKTREE="$TMP_DIR/source"
IMAGE="stellar-wrap-verify-$(date +%s)"

cleanup() {
  git -C "$ROOT" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
  docker image rm "$IMAGE" >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$TMP_DIR/download" "$TMP_DIR/out"
gh release download "$TAG" \
  --pattern stellar_wrap_contract.wasm \
  --pattern stellar_wrap_contract.wasm.sha256 \
  --dir "$TMP_DIR/download"

(cd "$TMP_DIR/download" && sha256sum --check stellar_wrap_contract.wasm.sha256)

git -C "$ROOT" fetch origin "refs/tags/$TAG:refs/tags/$TAG"
git -C "$ROOT" worktree add --detach "$WORKTREE" "$TAG"
docker build --platform linux/amd64 -t "$IMAGE" "$WORKTREE"
docker run --rm \
  -v "$TMP_DIR/out:/out" \
  "$IMAGE"

RELEASE_SHA="$(sha256sum "$TMP_DIR/download/stellar_wrap_contract.wasm" | awk '{print $1}')"
LOCAL_SHA="$(sha256sum "$TMP_DIR/out/stellar_wrap_contract.wasm" | awk '{print $1}')"
printf 'Release SHA256: %s\nLocal SHA256:   %s\n' "$RELEASE_SHA" "$LOCAL_SHA"
if [[ "$RELEASE_SHA" != "$LOCAL_SHA" ]]; then
  echo "Release artifact does not match the reproducible tag build." >&2
  exit 1
fi
echo "Release artifact matches the local build for $TAG."