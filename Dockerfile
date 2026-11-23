FROM --platform=linux/amd64 rust:1.94.1-slim-bookworm@sha256:f845b308e3104231c98dba2dab952ebd54e35cd6ab8ef4a230ecb38e2315f20c

# Pin the WASM optimizer version so optimizer output is deterministic.
ARG WASM_OPT_VERSION=0.116.1

# Install the exact toolchain declared in rust-toolchain.toml and the WASM target.
RUN rustup target add wasm32-unknown-unknown

# Install a pinned wasm-opt for deterministic post-processing of the WASM artifact.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) wasm_opt_arch=x86_64 ;; \
      arm64) wasm_opt_arch=aarch64 ;; \
      *) echo "unsupported arch: $arch" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/binaryen.tar.gz \
      "https://github.com/WebAssembly/binaryen/releases/download/version_${WASM_OPT_VERSION}/binaryen-version_${WASM_OPT_VERSION}-${wasm_opt_arch}-linux.tar.gz"; \
    tar -xzf /tmp/binaryen.tar.gz -C /tmp; \
    install -m 0755 "/tmp/binaryen-version_${WASM_OPT_VERSION}/bin/wasm-opt" /usr/local/bin/wasm-opt; \
    rm -rf /tmp/binaryen.tar.gz "/tmp/binaryen-version_${WASM_OPT_VERSION}"; \
    wasm-opt --version

WORKDIR /contract
COPY . .

# Build the WASM artifact and run it through the pinned optimizer so that a
# local build and a CI build of the same commit agree byte for byte.
RUN set -eux; \
    cargo build --release --target wasm32-unknown-unknown; \
    wasm-opt -Oz \
      target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm \
      -o target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm; \
    sha256sum target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm

ENTRYPOINT ["sh", "-c", "cp /contract/target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm /out/stellar_wrap_contract.wasm"]
