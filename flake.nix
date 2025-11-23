{
  description = "Ripdoc generates skeletonized outlines of Rust crates.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
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
          version = "0.2.0";
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
          ];

          buildInputs = with pkgs; [
            openssl
          ];
          cargoBuildFlags = [
            "-p"
            "ripdoc-cli"
          ];
          doCheck = false;
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
              (rust-bin.nightly.latest.default.override { extensions = [ "rust-src" ]; })
            ];

            shellHook = '''';
          };
      }
    );
}
