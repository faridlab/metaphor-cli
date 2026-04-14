#!/usr/bin/env bash
# Install script for metaphor-cli.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
#
# Environment variables:
#   METAPHOR_INSTALL_DIR   Install location (default: $HOME/.local/bin)
#   METAPHOR_VERSION       Release tag to install (default: latest)

set -euo pipefail

REPO="faridlab/metaphor-cli"
BIN="metaphor"
INSTALL_DIR="${METAPHOR_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${METAPHOR_VERSION:-latest}"

err() { echo "error: $*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || err "$1 is required"; }

detect_target() {
  local os arch
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Darwin) os="apple-darwin" ;;
    Linux)  os="unknown-linux-gnu" ;;
    *) err "unsupported OS: $os" ;;
  esac
  case "$arch" in
    x86_64|amd64)   arch="x86_64" ;;
    arm64|aarch64)  arch="aarch64" ;;
    *) err "unsupported arch: $arch" ;;
  esac
  echo "${arch}-${os}"
}

main() {
  need curl
  need tar
  need uname

  local target url tmp asset
  target=$(detect_target)
  asset="${BIN}-${target}.tar.gz"

  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/${REPO}/releases/latest/download/${asset}"
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
  fi

  tmp=$(mktemp -d)
  # Use :- so the EXIT trap doesn't trip `set -u` after main()'s local goes
  # out of scope.
  trap 'rm -rf "${tmp:-}"' EXIT

  echo "Downloading ${BIN} (${target})..."
  if ! curl -fsSL "$url" -o "$tmp/$asset"; then
    err "failed to download $url"
  fi

  tar -xzf "$tmp/$asset" -C "$tmp"

  mkdir -p "$INSTALL_DIR"
  mv "$tmp/$BIN" "$INSTALL_DIR/$BIN"
  chmod +x "$INSTALL_DIR/$BIN"

  echo "Installed ${BIN} to ${INSTALL_DIR}/${BIN}"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      echo
      echo "Note: $INSTALL_DIR is not on your PATH."
      echo "Add this to your shell profile:"
      echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac
}

main "$@"
