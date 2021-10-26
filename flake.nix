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
    in rec {
      defaultApp = flake-utils.lib.mkApp {
        drv = defaultPackage;
      };
      defaultPackage = (with pkgs; stdenv.mkDerivation {
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
            cargoSha256 = "sha256-VaBu/oyNoPePSTtbBsrFBEhOtFx9C09GtIr1K9HqQpY=";
          })
          (python39.withPackages(ps: with ps; [
            simplejson
            six
          ]))
        ];
        SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
        outputHash = "sha256-vaxFq7E7nCI2UoHMsNu8fGjEKzMiqJgfgPBi/ercMtw=";
        outputHashMode = "recursive";
        buildPhase = ''
          python vsdownload.py --accept-license --dest $TMP Microsoft.VisualStudio.VC.Llvm.Clang Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK.19041
        '';
        installPhase = ''
          mkdir $out
          windows_sdk --source $TMP --destination $out
        '';
      });
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          cachix
          rust
        ];
      };
    }
  );
}
