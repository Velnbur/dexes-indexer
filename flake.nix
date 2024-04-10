{
  description = "Decentralized exchanges indexer";

  inputs = {
    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.tar.gz";
    flake-parts.url = "github:hercules-ci/flake-parts";

    nixpkgs.url = "github:NixOS/nixpkgs/23.11";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    services-flake.url = "github:juspay/services-flake";
  };

  # nixConfig = {
  #   extra-trusted-public-keys =
  #     "eigenvalue.cachix.org-1:ykerQDDa55PGxU25CETy9wF6uVDpadGGXYrFNJA3TUs=";
  #   extra-substituters = "https://eigenvalue.cachix.org";
  #   allow-import-from-derivation = true;
  # };

  outputs = inputs@{ flake-parts, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "aarch64-darwin" ];

      imports = [
        inputs.process-compose-flake.flakeModule
        ./nix/rust-overlay/flake-module.nix
      ];

      perSystem = { self', config, pkgs, system, ... }:
        let
          customBuildRustCrateForPkgs = pkgs:
            pkgs.buildRustCrate.override {
              defaultCrateOverrides = pkgs.defaultCrateOverrides // {
                scale-info = attrs: { CARGO = 1; };
              };
            };

          cargoNix = pkgs.callPackage ./Cargo.nix {
            buildRustCrateForPkgs = customBuildRustCrateForPkgs;
          };

          dev-db = import ./nix/dev-db.nix { };

          postgresPkg = pkgs.postgresql;
        in {
          process-compose."default" = {
            imports = [ inputs.services-flake.processComposeModules.default ];

            services.postgres."dex-pg" = dev-db.nixos postgresPkg;
          };

          packages = rec {
            bootstrapper = cargoNix.workspaceMembers.bootstrapper.build;
            default = bootstrapper;
          };

          apps = rec {
            bootstrapper = {
              type = "app";
              program = "${self'.packages.bootstrapper}/bin/bootstrapper";
            };
            default = pkgs.writeScriptBin "dex" ''
              echo "Running dex..."
            '';
          };

          devShells.default = pkgs.mkShell {
            buildInputs = [ postgresPkg ] ++ (with pkgs; [ rust-toolchain ]);
            DATABASE_URL = dev-db.url;
          };

          devShells.paper =
            pkgs.mkShell { buildInputs = (with pkgs; [ texliveFull ]); };

          devShells.python = pkgs.mkShell {
            buildInputs = (with pkgs; [
              (callPackage ./nix/python.nix { })
              nodePackages.pyright
              graphviz
            ]);
            venvDir = "./.venv";
          };
        };
    };
}
