{ system, fetchurl, stdenv, pkgs, lib, ... }:
let
  version = "nightly-84d98427e17370ff08ec34f00e3c7e539753a760";

  fetchFromGithubAssets = { author, repo, version, asset, sha256 }: fetchurl {
    inherit sha256;
    url = "https://github.com/${author}/${repo}/releases/download/${version}/${asset}";
  };

  matrix = {
    aarch64-linux = { system = "linux_arm64"; sha256 = ""; };
    x86_64-linux = { system = "linux_amd64"; sha256 = ""; };
    aarch64-darwin = { system = "darwin_arm64"; sha256 = "sha256-/azNItOL0cGJgYZhWKoaMmkQ38JRbFWdyaXjA4W3TNY="; };
    x86_64-darwin = { system = "darwin_amd64"; sha256 = ""; };
  };

  pair = matrix.${system};
in
stdenv.mkDerivation rec {
  inherit version;
  name = "foundry-${version}";
  pname = "foundry";

  src = fetchFromGithubAssets {
    inherit version;
    author = "foundry-rs";
    repo = "foundry";
    asset = "foundry_nightly_${pair.system}.tar.gz";
    sha256 = pair.sha256;
  };

  phases = [ "installPhase" ];

  # buildInputs = [ pkgs.tar ];

  installPhase = ''
    mkdir -p $out/bin
  
    tar -xzf $src -C $out/bin

    chmod +x $out/bin/*
  '';
}
