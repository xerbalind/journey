{
  description = "Journey";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };
  outputs = {self,  nixpkgs, flake-utils, rust-overlay, ... }:
  flake-utils.lib.eachDefaultSystem (system:
  let
    overlays = [ (import rust-overlay) ];
    pkgs = import nixpkgs {
      inherit system overlays;
    };
  in
  with pkgs;
  {
    devShell = mkShell {
      buildInputs = [
        (rust-bin.stable.latest.default.override { extensions = [ "rust-analyzer-preview" "rust-src" "clippy"]; })
	pkg-config
        diesel-cli
	postgresql
	libpq
      ];

      LD_LIBRARY_PATH = lib.makeLibraryPath [ libpq ];
    };
  });
}
