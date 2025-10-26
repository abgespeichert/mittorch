#!/bin/bash
set -e

docker run --rm \
  -v "$PWD":/app \
  -w /app \
  rust:alpine3.20 \
  sh -c '
    set -e
    rustup target add aarch64-unknown-linux-musl
    apk add --no-cache build-base musl-dev
    cargo build --release --target aarch64-unknown-linux-musl
  '

echo "congratulations!"
echo "the build was successful"