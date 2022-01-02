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
        buildInputs = with pkgs; [
          (lib.optional stdenv.isDarwin darwin.Security)
          libiconv
        ];
        propagatedBuildInputs = with pkgs; [
          msitools
          unzip
        ];
        cargoSha256 = "sha256-HenIBLXFoY4y0kg2Pee8lJGv2xdyC+Go4tCCt2Fk4xc=";
      };
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          (pkgs.lib.optional pkgs.stdenv.isDarwin pkgs.darwin.Security)
          jq
          libiconv
          msitools
          rust
          unzip
          xh
        ];
      };
    }
  );
}
