{ rustBuilder, rustVersion, rustPkgs ? (rustBuilder.makePackageSet {
  inherit rustVersion;
  packageFun = import ../../Cargo.nix;
}), ... }:
(rustPkgs.workspace.bootstrapper { }).bin
