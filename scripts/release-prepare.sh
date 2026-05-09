#!/usr/bin/env bash
# Bumps version strings across the project and produces tauri bundles.
# Invoked by semantic-release's prepareCmd with the next semver string.
set -euo pipefail

VERSION="${1:?usage: release-prepare.sh <version>}"

echo "[release-prepare] bumping to $VERSION"

# package.json
node -e "
  const fs = require('fs');
  const p = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  p.version = process.argv[1];
  fs.writeFileSync('package.json', JSON.stringify(p, null, 2) + '\n');
" "$VERSION"

# src-tauri/tauri.conf.json
node -e "
  const fs = require('fs');
  const p = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
  p.version = process.argv[1];
  fs.writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(p, null, 2) + '\n');
" "$VERSION"

# src-tauri/Cargo.toml — first `version = "..."` line under [package]
python3 - "$VERSION" <<'PY'
import re, sys, pathlib
version = sys.argv[1]
path = pathlib.Path("src-tauri/Cargo.toml")
text = path.read_text()
new = re.sub(r'(?m)^(version\s*=\s*)"[^"]*"', f'\\1"{version}"', text, count=1)
path.write_text(new)
PY

echo "[release-prepare] building tauri bundles"
# tauri build runs cargo, which will refresh Cargo.lock with the new version
# automatically — no separate `cargo update` needed.
pnpm tauri build --bundles deb,rpm,appimage
