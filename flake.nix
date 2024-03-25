{
  description = "Decentralized exchanges indexer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";

    cargo2nix = {
      url = "github:cargo2nix/cargo2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, flake-utils, nixpkgs, rust-overlay, cargo2nix, ... }:
    let
      systems =
        [ "x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin" ];
    in flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays =
            [ rust-overlay.overlays.default cargo2nix.overlays.default ];
        };

        inherit (pkgs) lib stdenv;

        foundry = pkgs.callPackage ./nix/foundry.nix { };

        rustVersion = "1.75.0";

        rust-toolchain =
          pkgs.rust-bin.stable."${rustVersion}".default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" "rust-analyzer" ];
          };

        rustPkgs = pkgs.rustBuilder.makePackageSet {
          inherit rustVersion;

          packageFun = import ./Cargo.nix;
        };

        cli = (rustPkgs.workspace.cli { });

        darwinPkgs = (lib.optional stdenv.isDarwin (with pkgs; [
          libiconv
          darwin.apple_sdk.frameworks.SystemConfiguration
        ]));

        pythonWithPackages = pkgs.python311.withPackages (ps:
          with ps; [
            # tools
            python
            pip
            setuptools
            wheel
            virtualenv
            venvShellHook
            black
            # libraries
            graphviz
            psycopg2
          ]);
      in rec {
        packages = {
          inherit cli;
          default = packages.cli;
        };

        apps = {
          cli = flake-utils.lib.mkApp { drv = packages.cli; };
          default = apps.cli;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = (with pkgs; [
            sqlx-cli
            texliveFull
            foundry
            nodePackages.pyright
            graphviz
          ]) ++ [ rust-toolchain pythonWithPackages ] ++ darwinPkgs;

          venvDir = "./.venv";
          ETH_RPC_URL = "127.0.0.1:8545";
          DATABASE_URL = "postgresql://dex:admin123@127.0.0.1:5432/dex";
        };
      });
}
