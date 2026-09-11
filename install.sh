#!/usr/bin/env bash
# Install script for metaphor-cli.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
#
# Installs everything needed for the interactive terminal in one shot:
#   1. the `metaphor` binary
#   2. the `metaphor-ui` npm package that powers the interactive UI
#   3. — only when the machine has no Node.js >= 20 — a private Node.js
#      runtime under ~/.metaphor/node, used solely by the UI. It never
#      overwrites or shadows any existing node/npm installation.
#
# Environment variables:
#   METAPHOR_INSTALL_DIR   Install location (default: $HOME/.local/bin)
#   METAPHOR_VERSION       Release tag to install (default: latest)
#   METAPHOR_NODE_VERSION  Node.js version for the private runtime (default: 22.14.0)

set -euo pipefail

REPO="faridlab/metaphor-cli"
BIN="metaphor"
UI_PACKAGE="@metaphor/metaphor-ui"
INSTALL_DIR="${METAPHOR_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${METAPHOR_VERSION:-latest}"
NODE_VERSION="${METAPHOR_NODE_VERSION:-22.14.0}"
NODE_DIR="$HOME/.metaphor/node"

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

# Major Node.js version of a given node binary, or 0 when absent/unreadable.
node_major_of() {
  if [ -x "$1" ]; then
    "$1" -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0
  else
    echo 0
  fi
}

# True when a usable Node.js >= 20 is already on PATH.
node_on_path_ok() {
  command -v node >/dev/null 2>&1 || return 1
  [ "$(node_major_of "$(command -v node)")" -ge 20 ]
}

# Download the SHASUMS256.txt entry for a nodejs.org dist file and verify a
# local file against it. Uses sha256sum (linux) or shasum -a 256 (macOS).
verify_node_sha256() { # <local-file> <dist-filename>
  local file="$1" name="$2" expected actual
  expected=$(curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" \
    | grep " ${name}\$" | awk '{print $1}') || return 1
  [ -n "$expected" ] || return 1
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  else
    return 1
  fi
  [ "$actual" = "$expected" ]
}

# Install a private Node.js runtime under $NODE_DIR for machines without
# Node >= 20. Fails (non-zero) on any problem; callers print a hint.
bootstrap_node() {
  local os arch plat tarball
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Darwin) plat="darwin" ;;
    Linux)  plat="linux" ;;
    *) return 1 ;;
  esac
  case "$arch" in
    x86_64|amd64)  arch="x64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) return 1 ;;
  esac
  tarball="node-v${NODE_VERSION}-${plat}-${arch}.tar.gz"

  echo "Downloading Node.js v${NODE_VERSION} (${plat}-${arch}) for the interactive UI..."
  curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/${tarball}" -o "$tmp/$tarball" || return 1
  verify_node_sha256 "$tmp/$tarball" "$tarball" || return 1
  rm -rf "$NODE_DIR"
  mkdir -p "$NODE_DIR"
  tar -xzf "$tmp/$tarball" -C "$NODE_DIR" --strip-components=1 || return 1
  [ "$(node_major_of "$NODE_DIR/bin/node")" -ge 20 ] || return 1
}

# Launcher shim so metaphor-ui runs on the private Node runtime without the
# user ever needing `node` on their PATH.
write_ui_wrapper() {
  cat > "$INSTALL_DIR/metaphor-ui" <<WRAPPER
#!/bin/sh
exec "$NODE_DIR/bin/node" "$NODE_DIR/lib/node_modules/@metaphor/metaphor-ui/dist/main.js" "\$@"
WRAPPER
  chmod +x "$INSTALL_DIR/metaphor-ui"
}

ui_late_hint() {
  cat >&2 <<HINT

Note: installing ${UI_PACKAGE} failed — the interactive UI is skipped.
metaphor itself works without it. To add the UI later, run:
  npm install -g ${UI_PACKAGE}
HINT
}

# Install the interactive UI. Never fatal: the binary alone is fully usable,
# so any failure here prints the manual command instead of failing the script.
install_ui() {
  if command -v metaphor-ui >/dev/null 2>&1; then
    echo "Interactive UI (metaphor-ui) already installed — skipping"
    return 0
  fi

  local npm_bin="" private_node=0
  if node_on_path_ok; then
    npm_bin="npm"
  elif bootstrap_node; then
    npm_bin="$NODE_DIR/bin/npm"
    private_node=1
  else
    cat >&2 <<HINT

Note: the interactive terminal UI needs Node.js >= 20, which could not be
set up automatically. metaphor itself works without it. To add the UI later,
install Node.js 20+ and run:
  npm install -g ${UI_PACKAGE}
HINT
    return 0
  fi

  echo "Downloading the interactive UI (${UI_PACKAGE}) via npm..."
  if ! "$npm_bin" install -g --no-fund --no-audit "$UI_PACKAGE" >/dev/null 2>&1; then
    # A system npm can fail for environment reasons (permissions, odd global
    # prefix). Fall back to the private runtime once before giving up.
    if [ "$private_node" -eq 0 ] && bootstrap_node; then
      npm_bin="$NODE_DIR/bin/npm"
      private_node=1
      if ! "$npm_bin" install -g --no-fund --no-audit "$UI_PACKAGE" >/dev/null 2>&1; then
        ui_late_hint
        return 0
      fi
    else
      ui_late_hint
      return 0
    fi
  fi

  if command -v metaphor-ui >/dev/null 2>&1; then
    echo "Installed the interactive UI (${UI_PACKAGE})"
    return 0
  fi

  # The UI landed in the private runtime — expose a launcher shim from the
  # install dir so `metaphor ui` (and the user) can start it directly.
  if [ -f "$NODE_DIR/lib/node_modules/@metaphor/metaphor-ui/dist/main.js" ]; then
    write_ui_wrapper
    echo "Installed the interactive UI (${UI_PACKAGE})"
  else
    ui_late_hint
  fi
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
