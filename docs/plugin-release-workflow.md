# Plugin release workflow

Template GitHub Actions workflow for plugin repos. Drop this at `.github/workflows/release.yml` in each of:

- `faridlab/metaphor-plugin-dev`
- `faridlab/metaphor-plugin-schema`
- `faridlab/metaphor-plugin-codegen`
- `faridlab/metaphor-skill-agents`

Change **only** the `BIN` value to the plugin's binary name (`metaphor-dev`, `metaphor-schema`, `metaphor-codegen`, `metaphor-agent`). Everything else is shared.

Pushing a tag like `v0.1.0` produces four release assets — one per target — with names like `metaphor-dev-aarch64-apple-darwin.tar.gz`, each containing the bare binary at the tarball root. That is exactly the contract `metaphor plugin add` expects (see [plugins.md § Release asset contract](plugins.md#release-asset-contract)).

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  # One leader job creates the empty GitHub Release so the per-target
  # upload jobs don't race on "who creates it first."
  create-release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: taiki-e/create-gh-release-action@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  upload-assets:
    needs: create-release
    name: ${{ matrix.target }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          # macOS: both targets build on an ARM runner. The x86_64 build is a
          # cross-compile via Apple's SDK, which Rust + clang handle natively.
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v6

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Upload binary
        uses: taiki-e/upload-rust-binary-action@v1
        with:
          # CHANGE PER PLUGIN:
          bin: metaphor-dev
          target: ${{ matrix.target }}
          archive: $bin-$target
          token: ${{ secrets.GITHUB_TOKEN }}
```

## Notes on the `archive` value

`$bin-$target` expands to e.g. `metaphor-dev-aarch64-apple-darwin`, and `upload-rust-binary-action` appends `.tar.gz`. The resulting asset name matches `metaphor plugin add`'s expectation exactly. Don't change `archive` — changing it will break install.

## First release

1. Copy this workflow to the plugin repo as `.github/workflows/release.yml`, change `bin:`.
2. Commit and push.
3. Cut a tag:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
4. Watch the workflow — it creates a draft release, then four parallel jobs upload one asset each.
5. Verify with the CLI:
   ```bash
   metaphor plugin add metaphor-dev@0.1.0
   metaphor plugins   # ✓ metaphor-dev
   ```

## Troubleshooting

- **`download failed: <url>`** from `metaphor plugin add` → open `<url>` in a browser. If 404, either the tag doesn't exist or the asset is missing. Re-run the failing upload job.
- **`tarball did not contain '<name>' at its root`** → the tarball has a nested directory. This usually means `archive:` was customized — revert to `$bin-$target`.
