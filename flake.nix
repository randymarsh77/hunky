{
  description = "Hunky - A TUI for observing git changes in real-time";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };
        hunkyPackage = pkgs.rustPlatform.buildRustPackage {
          pname = "hunky";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          nativeCheckInputs = with pkgs; [ git ];
          buildInputs = with pkgs; [ openssl ];
        };
      in
      {
        packages = {
          default = hunkyPackage;
          hunky = hunkyPackage;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = hunkyPackage;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            cargo
            rustc
            rust-analyzer
            pkg-config
            openssl
            git
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          shellHook = ''
            echo "🦀 Rust development environment activated"
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
          '';
        };
      }
    );
}
