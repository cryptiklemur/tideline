#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "==> building plugins..."
cargo build --release -p tideline-tones -p tideline-notifications
echo "==> building host..."
cargo build --release -p tideline
echo "==> done. binary at: target/release/tideline"
