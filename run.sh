#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building..."
bash build.sh

echo "==> Launching Volta..."
love frontend/
