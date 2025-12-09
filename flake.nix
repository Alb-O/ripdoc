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
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "ripdoc";
          version = "0.2.2";
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
            clang
            llvmPackages_latest.bintools
          ];

          buildInputs = with pkgs; [
            openssl
            clang
            llvmPackages_latest.bintools
          ];
          cargoBuildFlags = [
            "-p"
            "ripdoc"
          ];
          doCheck = false;
          postInstall = ''
            wrapProgram $out/bin/ripdoc \
              --set OPENSSL_DIR ${pkgs.openssl.dev} \
              --set OPENSSL_LIB_DIR ${pkgs.openssl.out}/lib \
              --set OPENSSL_INCLUDE_DIR ${pkgs.openssl.dev}/include \
              --prefix PKG_CONFIG_PATH : ${pkgs.openssl.dev}/lib/pkgconfig \
              --set CC ${pkgs.clang}/bin/clang \
              --set CXX ${pkgs.clang}/bin/clang++ \
              --set AR ${pkgs.llvmPackages_latest.bintools}/bin/llvm-ar \
              --set RANLIB ${pkgs.llvmPackages_latest.bintools}/bin/llvm-ranlib \
              --set LD ${pkgs.llvmPackages_latest.bintools}/bin/ld.lld
          '';
          meta = with pkgs.lib; {
            description = "Query and outline Rust documentation from the command line";
            homepage = "https://github.com/Alb-O/ripdoc";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "ripdoc";
          };
        };

        devShells.default =
          with pkgs;
          mkShell {
            buildInputs = [
              openssl
              pkg-config
              cargo-sort
              clang
              llvmPackages_latest.bintools
              (rust-bin.nightly.latest.default.override { extensions = [ "rust-src" ]; })
            ];

            shellHook = ''
              export OPENSSL_DIR=${openssl.dev}
              export OPENSSL_LIB_DIR=${openssl.out}/lib
              export OPENSSL_INCLUDE_DIR=${openssl.dev}/include
              export PKG_CONFIG_PATH=${openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:}$PKG_CONFIG_PATH
              export CC=${clang}/bin/clang
              export CXX=${clang}/bin/clang++
              export AR=${llvmPackages_latest.bintools}/bin/llvm-ar
              export RANLIB=${llvmPackages_latest.bintools}/bin/llvm-ranlib
              export LD=${llvmPackages_latest.bintools}/bin/ld.lld
            '';
          };
      }
    );
}
