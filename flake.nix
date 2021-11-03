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
      buildInputs = with pkgs; [
        cacert
        msitools
        (python39.withPackages(ps: with ps; [
          simplejson
          six
        ]))
        rust
      ];
    in rec {
      defaultPackage = (with pkgs; stdenv.mkDerivation {
        name = "windows_sdk";
        src = pkgs.fetchFromGitHub {
          owner = "mstorsjo";
          repo = "msvc-wine";
          rev = "12f63eca95dccbe94ee1802209fb0c68c529628d";
          hash = "sha256-BjX2EtYdz9vw1m9gqyOekcNLeYLryxwNT8L94/NeSWU=";
        };
        inherit buildInputs;
        SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
        outputHash = "sha256-I4UGDcrtmX/1TAQz89peXsroetZmCM+1b3XYqexv/VB=";
        outputHashMode = "recursive";
        buildPhase = ''
          python vsdownload.py --accept-license --dest $TMP Microsoft.VisualStudio.VC.Llvm.Clang Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK.19041
        '';
        installPhase = ''
          mkdir $out
          cargo run -- --source $TMP --destination $out
        '';
      });
      devShell = pkgs.mkShell {
        inherit buildInputs;
      };
    }
  );
}
