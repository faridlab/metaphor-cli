# Contributing

## Build

```bash
cargo build              # debug build of every crate in the workspace
cargo build --release    # optimized build
```

The binary lands at `target/debug/metaphor` (or `target/release/metaphor`).

## Test

```bash
cargo test               # all crates
cargo test -p metaphor-workspace
```

`metaphor-workspace` has the most coverage today. Adding tests as you touch other crates is welcome.

## Run a local build

```bash
./target/debug/metaphor --help
./target/debug/metaphor init --bare
```

Or install it onto your `$PATH`:

```bash
cargo install --path crates/metaphor-cli
```

## Workspace layout

```
metaphor-cli/
├── Cargo.toml                         workspace root
├── install.sh                         shell installer (downloads release tarball)
├── npm/                               npm package (postinstall fetches binary)
├── README.md                          repo entry point → docs/
├── docs/                              the manual
└── crates/
    ├── metaphor-cli/                  the binary + dispatcher
    │   └── src/
    │       ├── main.rs                clap commands + dispatch
    │       ├── cmd_sync.rs            metaphor sync (clone/update remotes)
    │       ├── cmd_add.rs             metaphor add (register + optional clone)
    │       └── plugin_env.rs          plugin binary lookup + passthrough helpers
    ├── metaphor-workspace/            metaphor.yaml + metaphor.lock schema + I/O
    │   └── src/lib.rs
    ├── metaphor-plugin-api/           GeneratorPlugin / ToolPlugin traits
    │   └── src/lib.rs
    └── metaphor-scaffold/             Phase 2 placeholder
        └── src/lib.rs
```

## Adding a new top-level subcommand

Two steps in `crates/metaphor-cli/src/main.rs`:

1. Add a variant to the `Command` enum (under the right plugin section). Use the same clap attributes as the existing passthrough commands (`trailing_var_arg`, `allow_external_subcommands`, `allow_hyphen_values`) if you're forwarding to a plugin.
2. Add a match arm in `main()` that calls either:
   - `plugin_env::passthrough(<binary>, <subcommand>, args)` — runs `<binary> <subcommand> <args…>`, **or**
   - `plugin_env::passthrough_raw(<binary>, args)` — runs `<binary> <args…>` with no inserted subcommand.

Then update [docs/cli-reference.md](cli-reference.md) so the mapping table stays accurate.

## Pointing at locally-built plugin binaries

While developing a plugin alongside `metaphor`, the easiest setup is:

```bash
mkdir -p ~/.metaphor/bin
ln -sf $(realpath ../metaphor-schema/target/debug/metaphor-schema)      ~/.metaphor/bin/
ln -sf $(realpath ../metaphor-codegen/target/debug/metaphor-codegen)    ~/.metaphor/bin/
ln -sf $(realpath ../metaphor-dev/target/debug/metaphor-dev)            ~/.metaphor/bin/
ln -sf $(realpath ../metaphor-skill-agents/target/debug/metaphor-agent) ~/.metaphor/bin/
export METAPHOR_PLUGIN_BIN_DIR=~/.metaphor/bin
```

Now any `cargo build` in a plugin repo immediately changes what `metaphor <command>` does on the next invocation — no reinstall.

If `METAPHOR_PLUGIN_BIN_DIR` is set but the requested binary isn't there, lookup falls back to a bare-name `$PATH` search. Useful when you only want to override one of the plugins.

## Code style

- Standard `cargo fmt`. CI (when added) will enforce it.
- `cargo clippy` clean is the goal.
- New errors in `metaphor-workspace` go through the `WorkspaceError` enum (`thiserror`); everything else uses `anyhow::Result` with `.context(...)` for human-readable chains.
- Top-of-file `//!` rustdoc on each module is the norm — please keep it up to date.

## Releasing (notes for maintainers)

- `install.sh` downloads from `https://github.com/faridlab/metaphor-cli/releases/...`. The asset naming convention is `metaphor-<arch>-<os>.tar.gz`, e.g. `metaphor-aarch64-apple-darwin.tar.gz`.
- The npm package's postinstall script fetches the matching tarball. Bump `npm/package.json` `version` in lockstep with crate releases.

### Cut a release

The release pipeline lives at [.github/workflows/release.yml](../.github/workflows/release.yml). It triggers on any tag matching `v*`.

```bash
# 1. Make sure main is clean and pushed
git status
git push

# 2. Bump the workspace version in Cargo.toml if needed, then commit:
#    (skip for the very first release)
# git add Cargo.toml && git commit -m "chore: release v0.2.0"

# 3. Tag the commit you want to release
git tag v0.1.7

# 4. Push the tag — this triggers the GitHub Actions build
git push origin v0.1.7
```

The workflow builds four target tarballs (`x86_64/aarch64 × linux-gnu/apple-darwin`), uploads them as release assets, and publishes the GitHub release. Once green, `curl -fsSL .../install.sh | bash` works.

**Verify after the workflow completes:**

```bash
# Watch the run
gh run watch

# Confirm the release has all four assets
gh release view v0.1.0

# Test the installer locally in a throwaway shell
METAPHOR_INSTALL_DIR=/tmp/metaphor-test bash -c '
  curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
  /tmp/metaphor-test/metaphor --version
'
```

**Rolling back a bad release:** `gh release delete v0.1.0 --yes && git push origin :refs/tags/v0.1.0`. Users who already ran `curl | bash` against that tag won't auto-rollback — cut a new patch release instead.
- `METAPHOR_VERSION=v0.x.y` lets users pin via the shell installer; ensure tags exist before announcing.
