{
  description = "Hunky - A TUI for observing git changes in real-time";

  nixConfig = {
    extra-substituters = [ "https://hunky.sh/cache" ];
    # To enable signature verification, generate a signing key pair with
    # nix-store --generate-binary-cache-key hunky-cache-1 private.pem public.pem
    # then add the private key as the NIX_SIGNING_KEY CI secret and uncomment:
    # extra-trusted-public-keys = [ "hunky-cache-1:<base64-public-key>" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    opencache = {
      url = "github:randymarsh77/OpenCache";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      opencache,
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
      rec {
        packages = {
          default = hunkyPackage;
          hunky = hunkyPackage;
          opencache = opencache.packages.${system}.default;
        };

        apps.default = {
          type = "app";
          program = "${hunkyPackage}/bin/hunky";
        };
        defaultApp = apps.default;
        defaultPackage = hunkyPackage;

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
