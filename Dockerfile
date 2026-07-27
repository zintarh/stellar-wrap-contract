FROM rust:1.77-slim

RUN rustup target add wasm32-unknown-unknown

WORKDIR /contract
COPY . .

RUN cargo build --release --target wasm32-unknown-unknown
