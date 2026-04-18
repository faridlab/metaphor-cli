# Install

Three install paths are supported. Pick the one that fits your environment.

## 1. Shell installer (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
```

The script downloads a prebuilt release tarball from GitHub Releases, extracts the `metaphor` binary, and copies it into your install directory.

### Supported platforms

| OS | Architectures |
| --- | --- |
| macOS (Darwin) | `x86_64`, `aarch64` (Apple Silicon) |
| Linux (glibc) | `x86_64`, `aarch64` |

The script picks the right tarball by reading `uname -s` and `uname -m`. Other targets exit with `unsupported OS` / `unsupported arch`.

### Environment variables

| Variable | Default | Effect |
| --- | --- | --- |
| `METAPHOR_INSTALL_DIR` | `$HOME/.local/bin` | Where the `metaphor` binary is placed. |
| `METAPHOR_VERSION` | `latest` | A release tag (e.g. `v0.2.0`) to pin to. `latest` resolves to the GitHub `releases/latest` redirect. |

### Example: pin a version into `/usr/local/bin`

```bash
curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh \
  | METAPHOR_INSTALL_DIR=/usr/local/bin METAPHOR_VERSION=v0.1.0 bash
```

### PATH

If your install dir isn't on `$PATH`, the script prints a one-line export to add to your shell profile:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Required tools

`curl`, `tar`, `uname`. The script aborts with a clear error if any are missing.

## 2. npm

```bash
npm install -g @metaphor/metaphor-cli
```

This installs a tiny JS shim plus a postinstall hook that downloads the matching native binary into the package's `dist/` directory. The `metaphor` command on your `$PATH` forwards to that binary.

- Package: `@metaphor/metaphor-cli` (MIT, version `0.1.0`)
- Requires: Node ≥ 18
- Supported `os`: `darwin`, `linux`
- Supported `cpu`: `x64`, `arm64`

If the postinstall download fails (offline, behind a proxy, unsupported platform), the package install fails. Use the shell installer or build from source as a fallback.

## 3. From source

```bash
git clone https://github.com/faridlab/metaphor-cli
cd metaphor-cli
cargo install --path crates/metaphor-cli
```

This builds in release mode and installs into `$CARGO_HOME/bin` (typically `~/.cargo/bin`).

For local development you usually want a debug build instead:

```bash
cargo build
./target/debug/metaphor --help
```

## Upgrade

- **Shell installer:** rerun the curl command. It overwrites the existing binary in place.
- **npm:** `npm update -g @metaphor/metaphor-cli`.
- **Source:** `git pull && cargo install --path crates/metaphor-cli --force`.

## Uninstall

- **Shell installer:** `rm "$METAPHOR_INSTALL_DIR/metaphor"` (or whichever path you chose).
- **npm:** `npm uninstall -g @metaphor/metaphor-cli`.
- **Source:** `cargo uninstall metaphor-cli`.

## Verify

```bash
metaphor --version
metaphor --help
```

Both should print without error. The banner reads `⚡ Metaphor CLI`.

## Plugin binaries

The `metaphor` binary on its own only implements `init` and `list`. The remaining subcommands shell out to plugin binaries (`metaphor-schema`, `metaphor-codegen`, `metaphor-dev`, `metaphor-agent`) that ship as separate projects. See [plugins.md](plugins.md) for how to install them and how the lookup works.
