#!/usr/bin/env bash
# Install script for metaphor-cli.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
#
# Installs the `metaphor` binary, and — when Node.js >= 20 is available — the
# `metaphor-ui` npm package that powers the interactive terminal UI. After the
# script finishes, running `metaphor` is all a user needs to do.
#
# Environment variables:
#   METAPHOR_INSTALL_DIR   Install location (default: $HOME/.local/bin)
#   METAPHOR_VERSION       Release tag to install (default: latest)

set -euo pipefail

REPO="faridlab/metaphor-cli"
BIN="metaphor"
UI_PACKAGE="@metaphor/metaphor-ui"
INSTALL_DIR="${METAPHOR_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${METAPHOR_VERSION:-latest}"

# Script-scope so the EXIT trap can see it regardless of function scope and
# the script doesn't trip `set -u` if the trap fires before main() runs.
tmp=""
trap '[ -n "$tmp" ] && rm -rf "$tmp"' EXIT

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

# Major Node.js version, or 0 when node is absent/unreadable.
node_major() {
  if command -v node >/dev/null 2>&1; then
    node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0
  else
    echo 0
  fi
}

# Install the interactive UI. Never fatal: the binary alone is fully usable,
# so any failure here prints the manual command instead of failing the script.
install_ui() {
  if command -v metaphor-ui >/dev/null 2>&1; then
    echo "Interactive UI (metaphor-ui) already installed — skipping"
    return 0
  fi

  local major
  major=$(node_major)
  if [ "$major" -lt 20 ]; then
    cat >&2 <<HINT

Note: the interactive terminal UI needs Node.js >= 20, which was not found.
metaphor itself works without it. To add the UI later, install Node.js 20+
(e.g. https://nodejs.org or your package manager) and run:
  npm install -g ${UI_PACKAGE}
HINT
    return 0
  fi

  echo "Downloading the interactive UI (${UI_PACKAGE}) via npm..."
  if ! npm install -g "${UI_PACKAGE}" >/dev/null 2>&1; then
    cat >&2 <<HINT

Note: installing ${UI_PACKAGE} failed — the interactive UI is skipped.
metaphor itself works without it. To add the UI later, run:
  npm install -g ${UI_PACKAGE}
HINT
    return 0
  fi

  if ! command -v metaphor-ui >/dev/null 2>&1; then
    cat >&2 <<HINT

Note: ${UI_PACKAGE} installed, but metaphor-ui is not on your PATH.
Add npm's global bin directory to PATH (find it with \`npm prefix -g\`,
then append /bin) to enable the interactive UI.
HINT
    return 0
  fi

  echo "Installed the interactive UI (${UI_PACKAGE})"
}

main() {
  need curl
  need tar
  need uname

  local target url asset
  target=$(detect_target)
  asset="${BIN}-${target}.tar.gz"

  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/${REPO}/releases/latest/download/${asset}"
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
  fi

  # `tmp` is declared at script scope so the EXIT trap can see it.
  tmp=$(mktemp -d)

  echo "Downloading ${BIN} (${target})..."
  if ! curl -fsSL "$url" -o "$tmp/$asset"; then
    err "failed to download $url"
  fi

  tar -xzf "$tmp/$asset" -C "$tmp"

  mkdir -p "$INSTALL_DIR"
  mv "$tmp/$BIN" "$INSTALL_DIR/$BIN"
  chmod +x "$INSTALL_DIR/$BIN"

  echo "Installed ${BIN} to ${INSTALL_DIR}/${BIN}"

  install_ui

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      echo
      echo "Note: $INSTALL_DIR is not on your PATH."
      echo "Add this to your shell profile:"
      echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac

  echo
  echo "All set — run \`metaphor\` to start."
  echo "Bare \`metaphor\` opens the interactive UI; every command also works directly."
}

main "$@"
