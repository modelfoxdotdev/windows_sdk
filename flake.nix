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
        latest.rustc
        latest.cargo
        latest.clippy-preview
        latest.rustfmt-preview
        latest.rust-std
        latest.rust-src
        rust-analyzer
      ]);
      sdk = (with pkgs; stdenv.mkDerivation {
        name = "windows_sdk";
        src = pkgs.fetchFromGitHub {
          owner = "mstorsjo";
          repo = "msvc-wine";
          rev = "12f63eca95dccbe94ee1802209fb0c68c529628d";
          hash = "sha256-BjX2EtYdz9vw1m9gqyOekcNLeYLryxwNT8L94/NeSWU=";
        };
        buildInputs = [
          cacert
          msitools
          ((pkgs.makeRustPlatform {
            rustc = rust;
            cargo = rust;
          }).buildRustPackage {
            pname = "windows_sdk";
            version = "0.1.0";
            src = ./.;
            doCheck = false;
            cargoSha256 = "sha256-mayOyMmidV7Bn+0caf2+shpAV7ytfz1E7d02IF+PdM0=";
          })
          (python39.withPackages(ps: with ps; [
            simplejson
            six
          ]))
        ];
        SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
        outputHash = "sha256-599N0xhg2I1GGiYFtmfCoQGm9Jfe8vysIErh3zcsn0A=";
        outputHashMode = "recursive";
        buildPhase = ''
          python vsdownload.py --accept-license --dest $TMP Microsoft.VisualStudio.VC.Llvm.Clang Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK.19041
        '';
        installPhase = ''
          mkdir $out
          windows_sdk --source $TMP --destination $out
        '';
      });
    in rec {
      defaultApp = flake-utils.lib.mkApp {
        drv = defaultPackage;
      };
      defaultPackage = sdk;
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          rust
        ];
      };
    }
  );
}
