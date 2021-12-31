{
  inputs = {
    nixpkgs = {
      url = "github:nixos/nixpkgs/nixos-unstable";
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
        nativeBuildInputs = with pkgs; [
          libiconv
        ];
        propagatedBuildInputs = with pkgs; [
          msitools
          unzip
        ];
        cargoSha256 = "sha256-XYs7FeJKaopAnDzsWhpLC+OJtJvw1l1rcoEiAq555vU=";
      };
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
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
