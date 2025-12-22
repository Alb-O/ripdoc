{
  description = "Query Rust docs and crate API from the command line";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        ripdocPkg = pkgs.rustPlatform.buildRustPackage {
          pname = "ripdoc";
          version = "0.8.0";
          src = pkgs.lib.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "cargo-manifest-0.19.1" = "sha256-sTHScYSlkCgYYpv9diaTnPfUBCDFuzjfkRRBi75F0g8=";
              "rustdoc-json-0.9.7" = "sha256-mJiY2/X6aeT1LQwbuBp4Cpw5xzz5MAC44U6paCcQ77I=";
            };
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            rust-bin.nightly.latest.default
            makeWrapper
            gcc
            binutils
          ];

          buildInputs = with pkgs; [
            openssl
            gcc
            binutils
          ];
          cargoBuildFlags = [
            "-p"
            "ripdoc"
          ];
          AR = "${pkgs.binutils}/bin/ar";
          RANLIB = "${pkgs.binutils}/bin/ranlib";
          LD = "${pkgs.binutils}/bin/ld";
          CC = "${pkgs.gcc}/bin/gcc";
          CXX = "${pkgs.gcc}/bin/g++";
          doCheck = false;
          postInstall = ''
            wrapProgram $out/bin/ripdoc \
              --set OPENSSL_DIR ${pkgs.openssl.dev} \
              --set OPENSSL_LIB_DIR ${pkgs.openssl.out}/lib \
              --set OPENSSL_INCLUDE_DIR ${pkgs.openssl.dev}/include \
              --prefix PKG_CONFIG_PATH : ${pkgs.openssl.dev}/lib/pkgconfig \
              --set CC ${pkgs.gcc}/bin/gcc \
              --set CXX ${pkgs.gcc}/bin/g++ \
              --set AR ${pkgs.binutils}/bin/ar \
              --set RANLIB ${pkgs.binutils}/bin/ranlib \
              --set LD ${pkgs.binutils}/bin/ld \
              --prefix PATH : ${pkgs.rust-bin.nightly.latest.default}/bin:${pkgs.gcc}/bin:${pkgs.binutils}/bin:${pkgs.pkg-config}/bin
          '';
          meta = with pkgs.lib; {
            description = "Query and outline Rust documentation from the command line";
            homepage = "https://github.com/Alb-O/ripdoc";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "ripdoc";
          };
        };
      in
      {
        packages.default = ripdocPkg;

        devShells.default =
          with pkgs;
          mkShell {
            packages = [ ripdocPkg ];
            buildInputs = [
              openssl
              pkg-config
              cargo-sort
              gcc
              binutils
              (rust-bin.nightly.latest.default.override { extensions = [ "rust-src" ]; })
            ];

            OPENSSL_DIR = openssl.dev;
            OPENSSL_LIB_DIR = "${openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${openssl.dev}/include";
            PKG_CONFIG_PATH = "${openssl.dev}/lib/pkgconfig";
            CC = "${gcc}/bin/gcc";
            CXX = "${gcc}/bin/g++";
            AR = "${binutils}/bin/ar";
            RANLIB = "${binutils}/bin/ranlib";
            LD = "${binutils}/bin/ld";

            shellHook = ''
              export PKG_CONFIG_PATH=${openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:}$PKG_CONFIG_PATH
              export AR=${binutils}/bin/ar
              export RANLIB=${binutils}/bin/ranlib
              export LD=${binutils}/bin/ld
            '';
          };
      }
    );
}
