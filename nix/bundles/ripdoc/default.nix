/**
  ripdoc bundle: Query Rust docs and crate API from the command line.

  Provides the ripdoc CLI with nightly Rust toolchain for rustdoc JSON generation.
*/
{
  __functor =
    _:
    {
      pkgs,
      self',
      rootSrc,
      buildDeps,
      ...
    }:
    let
      inherit (buildDeps.rust) rustPlatform;

      buildInputs = [
        pkgs.openssl
        pkgs.pkg-config
        pkgs.gcc
        pkgs.binutils
      ];

      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.makeWrapper
        pkgs.gcc
        pkgs.binutils
      ];

      cargoToml = fromTOML (builtins.readFile (rootSrc + "/Cargo.toml"));
      version = cargoToml.package.version;

      commonEnv = {
        OPENSSL_DIR = "${pkgs.openssl.dev}";
        OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
        OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        AR = "${pkgs.binutils}/bin/ar";
        RANLIB = "${pkgs.binutils}/bin/ranlib";
        LD = "${pkgs.binutils}/bin/ld";
        CC = "${pkgs.gcc}/bin/gcc";
        CXX = "${pkgs.gcc}/bin/g++";
      };

      ripdocPkg = rustPlatform.buildRustPackage {
        pname = "ripdoc";
        inherit version;
        src = rootSrc;
        cargoLock = {
          lockFile = rootSrc + "/Cargo.lock";
          outputHashes = {
            "cargo-manifest-0.19.1" = "sha256-sTHScYSlkCgYYpv9diaTnPfUBCDFuzjfkRRBi75F0g8=";
            "rustdoc-json-0.9.7" = "sha256-mJiY2/X6aeT1LQwbuBp4Cpw5xzz5MAC44U6paCcQ77I=";
          };
        };

        inherit buildInputs nativeBuildInputs;
        inherit (commonEnv)
          OPENSSL_DIR
          OPENSSL_LIB_DIR
          OPENSSL_INCLUDE_DIR
          AR
          RANLIB
          LD
          CC
          CXX
          ;

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
            --prefix PATH : ${buildDeps.rust.rustToolchain}/bin:${pkgs.gcc}/bin:${pkgs.binutils}/bin:${pkgs.pkg-config}/bin
        '';

        meta = with pkgs.lib; {
          description = "Query Rust docs and crate API from the command line";
          homepage = "https://github.com/Alb-O/ripdoc";
          license = licenses.mit;
          mainProgram = "ripdoc";
        };
      };
    in
    {
      __outputs.perSystem.packages = {
        default = ripdocPkg;
        ripdoc = ripdocPkg;
      };

      __outputs.perSystem.checks.build = self'.packages.default;

      __outputs.perSystem.devShells.ripdoc = pkgs.mkShell {
        packages = [
          ripdocPkg
          pkgs.cargo-sort
        ];
        buildInputs = buildInputs ++ [
          buildDeps.rust.rustToolchain
        ];

        inherit (commonEnv)
          OPENSSL_DIR
          OPENSSL_LIB_DIR
          OPENSSL_INCLUDE_DIR
          AR
          RANLIB
          LD
          CC
          CXX
          ;

        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

        shellHook = ''
          export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:}$PKG_CONFIG_PATH
          export AR=${pkgs.binutils}/bin/ar
          export RANLIB=${pkgs.binutils}/bin/ranlib
          export LD=${pkgs.binutils}/bin/ld
        '';
      };

      # Export build deps for other bundles
      __outputs.perSystem.buildDeps.ripdoc = {
        inherit buildInputs nativeBuildInputs;
        env = commonEnv;
        cargoOutputHashes = {
          "cargo-manifest-0.19.1" = "sha256-sTHScYSlkCgYYpv9diaTnPfUBCDFuzjfkRRBi75F0g8=";
          "rustdoc-json-0.9.7" = "sha256-mJiY2/X6aeT1LQwbuBp4Cpw5xzz5MAC44U6paCcQ77I=";
        };
      };
    };
}
