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
        ];
        cargoSha256 = "sha256-kC0g3bAhHN+TmTTj1CZWBk9PmYpJTBzcd/B1zafS64E=";
      };
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          jq
          msitools
          rust
          xh
					llvm_12
        ];
      };
    }
  );
}
