{
  description = "Decentralized exchanges indexer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, flake-utils, nixpkgs, rust-overlay, crane, ... }:
    let
      systems =
        [ "x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin" ];
    in flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        inherit (pkgs) lib stdenv;

        foundry = pkgs.callPackage ./nix/foundry.nix { };

        rust-toolchain = (pkgs.rust-bin.fromRustupToolchainFile
          ./rust-toolchain.toml).override {
            extensions =
              [ "rustfmt" "clippy" "rust-analyzer" "rust-src" "cargo" ];
          };

        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;

        sqlFilter = path: _type: null != builtins.match ".*sql$" path;
        jsonFilter = path: _type: null != builtins.match ".*json$" path;
        sqlOrCargoOrJson = path: type:
          (sqlFilter path type) || (craneLib.filterCargoSources path type)
          || (jsonFilter path type);

        src = lib.cleanSourceWith {
          src = craneLib.path ./.; # The original, unfiltered source
          filter = sqlOrCargoOrJson;
        };

        commonArgs = {
          inherit src;
          pname = "dex";
          strictDeps = true;

          buildInputs = [ pkgs.pkg-config ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
              libiconv
              darwin.apple_sdk.frameworks.SystemConfiguration
              darwin.apple_sdk.frameworks.Security
            ]);

          nativeBuildInputs = [ pkgs.sqlx-cli ];

          SQLX_OFFLINE = "1";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        bootstrapper = craneLib.buildPackage commonArgs // {
          pname = "bootstrapper";
          inherit cargoArtifacts;

          doCheck = false;
        };

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

        devComposeFilePath = ./infrastructure/dev/docker-compose.yaml;
      in rec {
        checks = { inherit bootstrapper; };

        packages = {
          inherit bootstrapper;
          default = packages.bootstrapper;

          setup-compose = pkgs.writeShellScriptBin "setup-compose" ''
            if ! colima status &> 2; then
                colima start
            fi

            docker compose --file ${devComposeFilePath} --project-directory . up -d
          '';

          enter-db = pkgs.writeShellScriptBin "enter-db" ''
            docker compose --file ${devComposeFilePath} --project-directory . exec db psql -U dex dex
          '';
        };

        apps = {
          bootstrapper = flake-utils.lib.mkApp { drv = packages.bootstrapper; };
          default = apps.bootstrapper;
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          packages = (with pkgs; [
            sqlx-cli
            texliveFull
            foundry
            nodePackages.pyright
            graphviz
            docker-client
          ]) ++ [ rust-toolchain pythonWithPackages ];

          venvDir = "./.venv";
          ETH_RPC_URL = "127.0.0.1:8545";
          DATABASE_URL = "postgresql://dex:admin123@127.0.0.1:5432/dex";

          FONTCONFIG_FILE = pkgs.makeFontsConf {
            fontDirectories = [ pkgs.times-newer-roman ];
          };
        };
      });
}
