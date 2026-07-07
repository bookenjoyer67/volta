#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building Rust core library..."
cargo build --release --manifest-path core/Cargo.toml

# Copy shared library to frontend/ where LÖVE can find it
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    cp target/release/libvolta_core.so frontend/
    echo "==> Copied libvolta_core.so to frontend/"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    cp target/release/libvolta_core.dylib frontend/
    echo "==> Copied libvolta_core.dylib to frontend/"
else
    echo "==> Unknown OS: $OSTYPE"
    exit 1
fi

echo "==> Building TUI binary..."
cargo build --release --manifest-path core/Cargo.toml --bin volta-tui
echo "==> TUI binary at target/release/volta-tui"

echo "==> Build complete."
