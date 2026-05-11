#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "==> building frontend..."
npm run build
echo "==> building plugins..."
cargo build --release \
  -p tideline-ptt \
  -p tideline-notifications \
  -p tideline-tones \
  -p tideline-effects
echo "==> building host..."
cargo build --release -p tideline
echo "==> done. binary at: target/release/tideline"
