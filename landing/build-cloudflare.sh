#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="$PWD/.cloudflare-tools/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN=1.93.1
export CARGO_INCREMENTAL=0

if ! command -v rustup >/dev/null; then
  case "$(uname -m)" in
    x86_64) rustup_target=x86_64-unknown-linux-gnu ;;
    aarch64) rustup_target=aarch64-unknown-linux-gnu ;;
    *) echo "Unsupported Cloudflare build architecture" >&2; exit 1 ;;
  esac
  mkdir -p .cloudflare-tools
  curl --fail --silent --show-error --location \
    "https://static.rust-lang.org/rustup/archive/1.28.2/$rustup_target/rustup-init" \
    --output .cloudflare-tools/rustup-init
  curl --fail --silent --show-error --location \
    "https://static.rust-lang.org/rustup/archive/1.28.2/$rustup_target/rustup-init.sha256" \
    --output .cloudflare-tools/rustup-init.sha256
  (cd .cloudflare-tools && sha256sum --check rustup-init.sha256)
  chmod +x .cloudflare-tools/rustup-init
  .cloudflare-tools/rustup-init -y --profile minimal --default-toolchain "$RUSTUP_TOOLCHAIN"
fi
rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --target wasm32-unknown-unknown

wasm_version=$(node --input-type=module -e '
  import fs from "node:fs";
  const block = fs.readFileSync("Cargo.lock", "utf8").split("[[package]]").find(block => block.includes("name = \"wasm-bindgen\""));
  const version = block?.match(/version = "([^"]+)"/)?.[1];
  if (!version) throw new Error("Missing locked wasm-bindgen version");
  console.log(version);
')
if ! command -v wasm-bindgen >/dev/null || [ "$(wasm-bindgen --version)" != "wasm-bindgen $wasm_version" ]; then
  cargo install wasm-bindgen-cli --version "=$wasm_version" --locked --root "$PWD/.cloudflare-tools"
fi
npm --prefix landing ci --omit=dev --no-audit --no-fund
npm --prefix landing run build
