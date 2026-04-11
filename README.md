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

## Install

**macOS / Linux (recommended):**

```bash
curl -fsSL https://raw.githubusercontent.com/faridlab/metaphor-cli/main/install.sh | bash
```

**npm:**

```bash
npm install -g @metaphor/metaphor-cli
```

**From source:**

```bash
cargo install --path crates/metaphor-cli
```

## Build

```bash
cargo build
```

## Usage

```bash
mkdir my-workspace && cd my-workspace
metaphor init
cat metaphor.yaml
```
