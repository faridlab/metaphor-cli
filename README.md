# metaphor-cli

The Metaphor orchestrator. A meta-CLI that manages a workspace of independent project repos and helps them work together.

## Status

**Foundation.** Workspace skeleton + `metaphor init` only.

## Workspace layout

```
metaphor-cli/
├── Cargo.toml                       workspace root
└── crates/
    ├── metaphor-cli/                the binary
    ├── metaphor-workspace/          metaphor.yaml schema + I/O
    ├── metaphor-scaffold/           clones starter repos (Phase 2)
    └── metaphor-plugin-api/         plugin trait surface (Phase 4)
```

## Build

```bash
cargo build
```

## Install locally

```bash
cargo install --path crates/metaphor-cli
```

## Usage

```bash
mkdir my-workspace && cd my-workspace
metaphor init
cat metaphor.yaml
```
