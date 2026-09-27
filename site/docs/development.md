---
sidebar_position: 5
---

# Development

`nix develop` provides the Rust toolchain, rust-analyzer and system dependencies. Without Nix, install Rust directly.

```bash
cargo build
cargo test
cargo run -- --repo /path/to/repo
```

## Coverage

```bash
cargo install cargo-llvm-cov --locked   # once

cargo cov        # summary
cargo cov-lcov   # lcov.info for CI tooling
cargo cov-html   # target/llvm-cov/html/index.html
```
