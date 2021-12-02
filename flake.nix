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
  outputs = { nixpkgs, flake-utils, fenix, ... }: flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;
      };
      rust = with fenix.packages.${system}; combine (with stable; [
        cargo
        clippy-preview
        rust-src
        rust-std
        rustc
        rustfmt-preview
      ]);
      cli = (pkgs.makeRustPlatform {
        rustc = rust;
        cargo = rust;
      }).buildRustPackage {
        name = "cli";
        src = ./.;
        doCheck = false;
        nativeBuildInputs = with pkgs; [
          libiconv
        ];
        cargoSha256 = "sha256-XYEMnL8+EdIDvH6GaRZO+VeDB3IoRW6JQn/odXrQnxg=";
      };
      download = (with pkgs; stdenv.mkDerivation {
        name = "download";
        src = pkgs.fetchFromGitHub {
          owner = "mstorsjo";
          repo = "msvc-wine";
          rev = "12f63eca95dccbe94ee1802209fb0c68c529628d";
          hash = "sha256-BjX2EtYdz9vw1m9gqyOekcNLeYLryxwNT8L94/NeSWU=";
        };
        buildInputs = [
          cacert
          msitools
          (python39.withPackages(ps: with ps; [
            simplejson
            six
          ]))
        ];
        installPhase = ''
          mkdir $out && python vsdownload.py --accept-license --dest $out Microsoft.VisualStudio.VC.Llvm.Clang Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK.19041
        '';
        outputHashMode = "recursive";
        outputHash = "sha256-dUGaoct0QLyqUm2v37HXFJGk0MCIshXpxfYw9Fw3rl8=";
      });
      windows_sdk = pkgs.runCommand "windows_sdk" {
        buildInputs = [
          cli
          download
        ];
      } ''
        mkdir $out
        windows_sdk --source ${download} --destination $out
      '';
    in rec {
      defaultPackage = windows_sdk;
      devShell = pkgs.mkShell {
        buildInputs = [
          rust
        ];
      };
    }
  );
}
