inputs@{
  flake-parts,
  systems,
  rust-overlay,
  treefmt-nix,
  ...
}:
flake-parts.lib.mkFlake { inherit inputs; } {
  systems = import systems;

  imports = [ treefmt-nix.flakeModule ];

  perSystem =
    {
      config,
      pkgs,
      ...
    }:
    let
      rootSrc = ./..;
      cargoToml = builtins.fromTOML (builtins.readFile (rootSrc + "/Cargo.toml"));
      version = cargoToml.package.version;

      rustPkgs = pkgs.extend rust-overlay.overlays.default;
      rustToolchain = rustPkgs.rust-bin.fromRustupToolchainFile (rootSrc + "/rust-toolchain.toml");
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
      };

      cargoSortWrapper = pkgs.writeShellScriptBin "cargo-sort-wrapper" ''
        set -euo pipefail

        opts=()
        files=()

        while [[ $# -gt 0 ]]; do
          case "$1" in
            --*) opts+=("$1"); shift ;;
            *) files+=("$1"); shift ;;
          esac
        done

        for f in "''${files[@]}"; do
          ${pkgs.lib.getExe pkgs.cargo-sort} "''${opts[@]}" "$(dirname "$f")"
        done
      '';

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

        doCheck = false;

        meta = with pkgs.lib; {
          description = "Query Rust docs and crate API from the command line";
          homepage = "https://github.com/Alb-O/ripdoc";
          license = licenses.mit;
          mainProgram = "ripdoc";
        };
      };

      rustDevPackages = [
        rustToolchain
        pkgs.rust-analyzer
        pkgs.cargo-watch
        pkgs.cargo-edit
      ];
    in
    {
      treefmt = {
        projectRootFile = "flake.nix";

        programs.rustfmt.enable = true;
        programs.nixfmt.enable = true;

        settings.formatter.cargo-sort = {
          command = "${cargoSortWrapper}/bin/cargo-sort-wrapper";
          options = [ "--workspace" ];
          includes = [
            "Cargo.toml"
            "**/Cargo.toml"
          ];
        };
      };

      packages = {
        default = ripdocPkg;
        ripdoc = ripdocPkg;
      };

      checks = {
        build = ripdocPkg;
        ripdoc = ripdocPkg;
      };

      devShells = {
        rust = pkgs.mkShell {
          packages = rustDevPackages;
        };

        ripdoc = pkgs.mkShell {
          packages = [
            ripdocPkg
            pkgs.cargo-sort
          ]
          ++ rustDevPackages;
        };

        default = pkgs.mkShell {
          packages = rustDevPackages ++ [
            pkgs.cargo-sort
            config.treefmt.build.wrapper
          ];
        };
      };
    };
}
