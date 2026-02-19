---
sidebar_position: 2
---

# Getting Started

## Prerequisites

- [Nix](https://nixos.org/) with flakes enabled (recommended)
- Or: Rust toolchain (cargo, rustc)

## Installation

### With Nix

```bash
# Enter the development environment
nix develop

# Build the project
cargo build --release

# Run it
cargo run --release
```

### Without Nix

Make sure you have Rust installed, then:

```bash
cargo build --release
cargo run --release
```

## Usage

Navigate to a git repository and run:

```bash
hunky

# or during development:
cargo run

# Specify a different repository:
hunky --repo /path/to/repo
```

## Try the Demo

In a separate terminal, start the simulation script:

```bash
./simulation.sh
```

Then build and run:

```bash
cargo run -- --repo test-repo
```

The simulation will continuously make file changes that Hunky will detect and display automatically!
