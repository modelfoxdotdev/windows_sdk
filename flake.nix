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
      rust = (with fenix.packages.${system}; combine [
        latest.cargo
        latest.clippy-preview
        latest.rust-src
        latest.rust-std
        latest.rustc
        latest.rustfmt-preview
        rust-analyzer
      ]);
      sdk = (with pkgs; stdenv.mkDerivation {
        name = "sdk";
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
        SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
        outputHash = "sha256-J30NavUAwqdzqfMHP0zhmiMc6NKHnqC7pyud7FmB6Io=";
        outputHashMode = "recursive";
        installPhase = ''
          mkdir $out && python vsdownload.py --accept-license --dest $out Microsoft.VisualStudio.VC.Llvm.Clang Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK.19041
        '';
      });
    in rec {
      defaultPackage = (with pkgs; stdenv.mkDerivation {
        pname = "windows_sdk";
        version = "0.0.0";
        src = ./.;
        buildInputs = [
          ((pkgs.makeRustPlatform {
            rustc = rust;
            cargo = rust;
          }).buildRustPackage {
              pname = "windows_sdk";
              version = "0.0.0";
              src = ./.;
              doCheck = false;
              cargoSha256 = "sha256-IoyYZuRoTDexXWTlI46KufQyJ5hSvZo2H0YdAeZkTOM=";
          })
          sdk ];
        installPhase = ''
          mkdir $out
          windows_sdk --source ${sdk} --destination $out
        '';
      });
      devShell = pkgs.mkShell {
        buildInputs = [ rust ];
      };
    }
  );
}
