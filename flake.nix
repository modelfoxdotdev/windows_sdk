{
  inputs = {
    nixpkgs = {
      url = "github:nixos/nixpkgs";
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    fenix = {
      url = "github:nix-community/fenix";
    };
  };
  outputs = inputs: inputs.flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import inputs.nixpkgs {
        inherit system;
      };
      rust = with inputs.fenix.packages.${system}; combine (with stable; [
        cargo
        clippy-preview
        rust-src
        rust-std
        rustc
        rustfmt-preview
      ]);
    in
    rec {
      defaultPackage = (pkgs.makeRustPlatform {
        rustc = rust;
        cargo = rust;
      }).buildRustPackage {
        name = "windows_sdk";
        src = ./.;
        doCheck = false;
        buildInputs = with pkgs; [
          (lib.optional stdenv.isDarwin darwin.Security)
          (lib.optional stdenv.isDarwin libiconv)
        ];
        propagatedBuildInputs = with pkgs; [
          msitools
          unzip
        ];
        cargoLock = { lockFile = ./Cargo.lock; };
      };
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          (lib.optional stdenv.isDarwin darwin.Security)
          (lib.optional stdenv.isDarwin libiconv)
          jq
          msitools
          rust
          unzip
          xh
        ];
      };
    }
  );
}
