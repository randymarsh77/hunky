---
sidebar_position: 2
---

# Getting Started

## Install

### Nix

```bash
# Run without installing
nix run github:randymarsh77/hunky
```

Or add `hunky.url = "github:randymarsh77/hunky";` to your flake inputs and use `hunky.packages.${system}.default`.

### Prebuilt binaries

Download from [Releases](https://github.com/randymarsh77/hunky/releases).

- macOS and Linux: `hunky-<arch>-apple-darwin.tar.gz` / `hunky-<arch>-unknown-linux-gnu.tar.gz`.
- Windows: `hunky-x86_64-pc-windows-msvc.zip` or `hunky-aarch64-pc-windows-msvc.zip`. The Linux binaries run on Windows only inside WSL.

### From source

Requires the Rust toolchain, or use `nix develop`.

```bash
cargo install --git https://github.com/randymarsh77/hunky
```

## Usage

```bash
hunky                        # watch the current repository
hunky --repo /path/to/repo   # watch another one
```

Press `H` for the key bindings sidebar, or `Shift+H` for extended help.

## Try the demo

`simulation.sh` clones a repository into `test-repo` and replays commits into it. In one terminal:

```bash
./simulation.sh
```

In another:

```bash
cargo run -- --repo test-repo
```
