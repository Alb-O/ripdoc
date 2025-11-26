# Building WASM Components on NixOS

## Problem Statement

Building Rust projects targeting `wasm32-wasip2` on NixOS presents several challenges due to the interaction between Rust's bundled linker wrappers, NixOS's modified toolchain paths, and the experimental nature of WASI Preview 2 support. Additionally, creating proper WASI command components requires understanding the distinction between core WASM modules and component model binaries.

## Initial Linking Failure

The first error encountered was a linker failure during the build of the `ring` dependency:

```
/nix/store/.../nix-support/ld-wrapper.sh: No such file or directory
collect2: error: ld returned 127 exit status
```

This occurs because Rust's bundled `gcc-ld` wrapper references Nix store paths from the rustup installation that don't exist in the current NixOS environment. The root cause is that rustup-installed toolchains embed absolute paths to Nix wrappers that are only valid in the environment where rustup was installed.

### Solution: Cargo Configuration

Create `.cargo/config.toml` to override the default linker behavior:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "cc"
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.wasm32-wasip2]
linker = "wasm-ld"
```

This configuration instructs Cargo to use the system's `cc` for native builds and `wasm-ld` for WASM targets, bypassing the problematic rustup-bundled wrappers.

## WASM Target Installation

After resolving the linker issue, the next error indicates missing WASM standard library:

```
error[E0463]: can't find crate for `core`
  = note: the `wasm32-wasip2` target may not be installed
```

In a NixOS environment using flakes, targets must be explicitly specified in the Rust toolchain override:

```nix
(rust-bin.nightly.latest.default.override {
  extensions = [ "rust-src" ];
  targets = [ "wasm32-wasip1" "wasm32-wasip2" ];
})
```

The `rust-src` extension is required for WASM targets as they compile the standard library from source.

## Dependency Incompatibility: TLS in WASM

Once the build infrastructure was functional, compilation failed due to the `ring` cryptography library attempting to use native assembly:

```
failed to resolve import `env::ring_core_0_17_14__p384_elem_mul_mont`
```

The dependency chain was: `ripdoc` → `ureq` → `rustls` → `ring`. The `ring` crate uses platform-specific assembly implementations for cryptographic operations, which cannot be compiled to WASM.

### Solution: Conditional Compilation

The network functionality using `ureq` was isolated to the `cargo_utils::registry` module. The solution involved making this functionality conditional on the target platform:

**Cargo.toml changes:**

```toml
[target.'cfg(not(target_family = "wasm"))'.dependencies]
ureq = { version = "3.1" }
```

**Source code changes:**

```rust
#[cfg(not(target_family = "wasm"))]
use ureq::http;

#[cfg(not(target_family = "wasm"))]
pub fn fetch_registry_crate(...) -> Result<CargoPath> {
    // Full implementation with network access
}

#[cfg(target_family = "wasm")]
pub fn fetch_registry_crate(...) -> Result<CargoPath> {
    // Stub returning error for network operations
    // Only local cache lookups work
}
```

This approach maintains the full API surface while providing appropriate error messages when network operations are attempted in WASM builds.

## Component Model Conversion

After successfully building the core WASM module, attempting to run it with wasmtime resulted in:

```
unknown import: `wasi:cli/environment@0.2.4::get-arguments` has not been defined
```

This error reveals a fundamental aspect of WASI Preview 2: Rust currently produces core WASM modules that import WASI 0.2.x interfaces, but these must be wrapped in the component model format for wasmtime to execute them.

### Understanding WASI Adapters

WASI adapters bridge the gap between WASI Preview 1 (what Rust's standard library actually uses internally) and WASI Preview 2 (the component model). Two types exist:

- **Reactor adapters**: For library-like components with exported functions
- **Command adapters**: For CLI applications with a main entry point

The critical distinction is that command adapters export the `wasi:cli/run` interface required by wasmtime's command execution model.

### Componentization Process

```bash
# Download the appropriate adapter
curl -L -o .wasm-adapters/wasi_snapshot_preview1.command.wasm \
  https://github.com/bytecodealliance/wasmtime/releases/download/v38.0.3/wasi_snapshot_preview1.command.wasm

# Convert core module to component
wasm-tools component new target/wasm32-wasip2/release/ripdoc.wasm \
  --adapt .wasm-adapters/wasi_snapshot_preview1.command.wasm \
  -o target/wasm32-wasip2/release/ripdoc-component.wasm
```

The adapter version must match the wasmtime version for compatible interface exports.

## Limitations of WASM Components

While the component builds and executes successfully for basic operations, fundamental limitations exist:

**Process spawning**: WASI does not support `std::process::Command`. Any functionality requiring external process execution (like running `cargo` or `rustc`) will fail with "operation not supported on this platform".

**Network I/O**: Standard networking via `ureq` or `reqwest` requires TLS libraries that depend on native code. WASI HTTP exists but is experimental and not widely supported by HTTP client libraries.

**File system access**: WASM components use capability-based security. Directories must be explicitly mounted via wasmtime's `--dir` flag:

```bash
wasmtime --dir=/path/to/data target/wasm32-wasip2/release/ripdoc-component.wasm print /data/crate
```

## Practical Workflow

For a tool like ripdoc that generates rustdoc JSON, the WASM component is suitable for:

1. Processing pre-generated rustdoc JSON files
2. Working with cached crates in the Cargo home directory
3. Performing searches and rendering on existing documentation

The WASM component cannot:

1. Generate new documentation (requires rustc/cargo)
2. Fetch crates from crates.io (network/TLS limitation)
3. Download crate metadata (network limitation)

## Efficient Build Process

The complete build process integrating all solutions:

```bash
# 1. Ensure Nix environment includes necessary tools
nix develop  # or direnv reload

# 2. Build the WASM module
cargo build --release --target wasm32-wasip2

# 3. Componentize
./componentize.sh target/wasm32-wasip2/release/ripdoc.wasm

# 4. Test
wasmtime target/wasm32-wasip2/release/ripdoc-component.wasm --help
```

## Alternative Approach: cargo-component

For projects designed from the start to be WASM components, `cargo-component` provides a more integrated workflow:

```bash
cargo component build --release
```

This tool automatically handles component model code generation, WIT interface definitions, and adapter linking. However, it requires restructuring existing projects to use the component model's interface types rather than standard Rust types.

## Future Considerations

**WASI Preview 3**: Under development with improved I/O capabilities and potentially better standard library support.

**wasi-http**: Experimental HTTP implementation for WASM components that could replace ureq, though ecosystem adoption remains limited.

**cargo-component**: As the component model matures, this will likely become the standard build tool, making manual adapter management unnecessary.

**Cross-compilation**: For true cross-platform distribution, native binaries for each major platform (Linux x64/ARM, macOS x64/ARM, Windows x64) still provide better functionality than WASM components for tools requiring system interaction.

## Verification

The successful build can be verified by inspecting the component's interface:

```bash
wasm-tools component wit target/wasm32-wasip2/release/ripdoc-component.wasm
```

This shows the imported WASI interfaces (I/O, filesystem, environment) and confirms the component exports the required `wasi:cli/run` interface.

## Performance Notes

WASM components have measurable overhead compared to native binaries:

- Startup time includes WASM compilation/validation
- Capability-based security checks on I/O operations
- Lack of SIMD optimizations in some implementations

For CLI tools, native compilation remains the optimal choice unless cross-platform distribution from a single artifact is a hard requirement.
