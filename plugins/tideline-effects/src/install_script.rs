/// Shell script invoked under pkexec. Detects distro from /etc/os-release and
/// runs the right package manager. Exits non-zero with stderr on failure so
/// the caller can surface the error.
pub const INSTALL_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
. /etc/os-release
case "${ID:-}${ID_LIKE:-}" in
  *arch*|*manjaro*|*endeavouros*)
    pacman -S --noconfirm carla lsp-plugins-vst3
    ;;
  *debian*|*ubuntu*|*pop*|*mint*|*elementary*)
    apt-get update -y
    apt-get install -y carla lsp-plugins
    ;;
  *fedora*|*rhel*|*centos*|*rocky*|*almalinux*)
    dnf install -y carla lsp-plugins
    ;;
  *)
    echo "Unsupported distro: ID=${ID:-?} ID_LIKE=${ID_LIKE:-?}" >&2
    exit 1
    ;;
esac
"#;
