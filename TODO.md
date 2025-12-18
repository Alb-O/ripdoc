# RipDoc Roadmap (keep this updated!)

Feel free to add more intermediate sub-tasks as needed.

- [x] organise into subcommands
- [x] show `// ...` between search results with large distance appart
- [ ] functionality/subcommands for fetching:
  - [ ] examples
  - [x] READMEs
- [x] 'with filename' support on module list (-l) show the originating .rs path for each
  - [x] json format support for module/symbol list
    - [ ] TOON-like compact format
  - [ ] ability to dump source code files easily (without having to know path of cache registry)
    - [ ] possibly filter out/exclude comments/docstrings on source code dump as the docs would likely already be in context
- [x] Allow 'or' searches, e.g. `ripdoc print gix --search "init|clone|fetch|remote|config"`
- [ ] Allow searching version numbers without specifying last digit e.g. `ripdoc print bat@0.24`

---

## Known Issue: Hang on Certain Crates (e.g., ratatui)

### Summary

When running `ripdoc print` on certain crates, the process hangs indefinitely without producing any output. This has been observed specifically with the `ratatui` crate in the `tome` workspace, while other crates in the same workspace work correctly.

### Symptoms

```bash
# Works fine (completes in <1s):
ripdoc print tome/lib/types --private
ripdoc print tome/lib/core --private
ripdoc print tome/lib/notifications --private

# Hangs indefinitely with no output:
ripdoc print tome/lib/ratatui --private
```

When running with `-v` (verbose), no cargo/rustdoc output is shown - the process simply hangs after startup.

### Environment

- NixOS with nightly Rust via Nix flake (no rustup)
- rustc 1.93.0-nightly (3ff30e7ea 2025-11-29)
- `rustup` is not available in PATH

### Investigation

#### Direct rustdoc works

Running `cargo rustdoc` directly on the problematic crate completes successfully in ~1 second:

```bash
cargo rustdoc --manifest-path tome/lib/ratatui/Cargo.toml -- --output-format json -Z unstable-options
# Completes with warnings in ~1s
```

#### strace analysis

Using strace to trace the hang:

```bash
strace -f -e trace=execve cargo run -- print tome/lib/ratatui --private
```

Observations:
1. ripdoc spawns and checks for `rustup --version` (fails with ENOENT as expected)
2. ripdoc then checks `rustc --version` (succeeds)
3. **No further execve calls are made** - the process just sits idle
4. No cargo or rustdoc commands are ever launched

The strace shows the process exits code 127 after the rustup check fails, then checks rustc version, then... nothing. It appears the `rustdoc-json` crate never proceeds to actually run `cargo rustdoc`.

#### Cache behavior

- Clearing `~/.cache/ripdoc/` does not resolve the issue
- Working crates (types, core, notifications) work with or without cache
- The problematic crate (ratatui) hangs with or without cache

#### Crate differences

The working crates are relatively simple. The `ratatui` crate:
- Has many dependencies (crossterm, itertools, etc.)
- Uses `edition = "2024"`
- Has complex feature flags

However, this shouldn't matter since direct `cargo rustdoc` works.

### Root Cause Hypothesis

The issue appears to be in the `rustdoc-json` crate (dependency of ripdoc) and how it handles the non-rustup case. When `rustup` is not available:

1. ripdoc correctly detects this via `is_rustup_available()` 
2. ripdoc skips setting `.toolchain("nightly")` on the builder
3. The `rustdoc-json` crate's `build_with_captured_output()` method appears to block indefinitely for certain crates

The fact that simpler crates work suggests there may be:
- A race condition or deadlock in output capturing
- An issue with how large dependency trees are handled
- A problem specific to certain Cargo.toml configurations

### Workarounds

1. Target specific packages instead of the full workspace:
   ```bash
   ripdoc print tome/lib/core --private  # works
   ```

2. Skip problematic crates when iterating workspaces

### Next Steps

1. Test with rustup available to see if the issue is specific to non-rustup environments
2. Add timeout handling to `read_crate()` to prevent indefinite hangs
3. Investigate the `rustdoc-json` crate's `build_with_captured_output()` implementation
4. Consider filing an upstream issue with the `rustdoc-json` crate
5. Add progress/status output so users know which package is being processed

### Related Changes

This issue was discovered while implementing multi-crate workspace support. The workspace iteration feature itself works correctly - when targeting a workspace root without specifying a package, ripdoc now:

1. Lists all workspace members
2. Iterates over each, generating documentation
3. Concatenates output with package headers and separators

The hang occurs during the documentation generation phase for specific crates, not in the workspace iteration logic.
