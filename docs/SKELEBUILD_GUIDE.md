# Ripdoc Skelebuild: Usage Guide

`skelebuild` allows you to incrementally construct tailored "source maps" of Rust crates by mixing high-level API signatures with full source implementations while preserving the original module hierarchy. It maintains state in the XDG state directory, enabling multi-session construction.

## Step-by-Step Example: Mapping the `bat` Crate

### 1. Initialize with Structural Outlines
Start by defining your output file and adding high-level structural items. By default, items are added as skeletons (signatures and fields only).

```bash
ripdoc skelebuild reset
ripdoc skelebuild add bat::config::Config --output bat_map.md
```

**Expected Output (`bat_map.md`):**
```rust
pub mod bat {
    pub mod config {
        pub struct Config<'a> {
            pub language: Option<&'a str>,
            pub colored_output: bool,
            // ... (other fields)
        }
    }
}
```

### 2. Inject Core Logic
When you need to see the internal implementation of a specific component, use the `--full` (or `-f`) flag. `ripdoc` will extract the actual source code and associated `impl` blocks from the disk.

```bash
ripdoc skelebuild add bat::controller::Controller::run --full
```

**Updated Output (`bat_map.md`):**
Note how `controller` is nested correctly alongside `config`, and the `run` method includes its original body and comments.

```rust
pub mod bat {
    pub mod config { /* ... previously added ... */ }

    pub mod controller {
        impl Controller<'_> {
            pub fn run(&self, inputs: Vec<Input>) -> Result<bool> {
                // extracted implementation...
                self.run_with_error_handler(inputs, output_handle, default_error_handler)
            }
        }
    }
}
```

### 3. Add Contextual Helpers
Mix in other relevant items. Adding a container (like a module or struct) without `--full` provides a quick reference for its public API.

```bash
# Add struct as skeleton for API reference
ripdoc skelebuild add bat::assets::HighlightingAssets

# Add utility function with full source
ripdoc skelebuild add bat::assets::get_acknowledgements --full
```

**Updated Output (`bat_map.md`):**
The `assets` module is now injected, containing both a signature-only struct and a fully-implemented function.

```rust
pub mod bat {
    pub mod assets {
        pub struct HighlightingAssets { /* ... fields ... */ }
        impl HighlightingAssets { /* ... signatures ... */ }

        pub fn get_acknowledgements() -> &'static str {
            "..." // full body
        }
    }
    // ... config and controller modules
}
```

## State Management

Review tracked targets or prune items as the skeleton evolves. State persists until `reset`.

```bash
# Check current targets and output path
ripdoc skelebuild status

# Remove a specific target
ripdoc skelebuild remove bat::assets::get_acknowledgements
```

## Features

- **Automatic Impl Tracking**: Adding a struct or enum with `--full` automatically marks its associated `impl` blocks for injection.
- **Hierarchical Merging**: Items are automatically grouped by parent modules and nested correctly, regardless of addition order.
- **Multiple Formats**: Toggle between Markdown documentation (default) and valid Rust code using `ripdoc skelebuild --format rust`.

## Technical Summary

`skelebuild` leverages `rustdoc` JSON to resolve item spans and `SearchIndex` to preserve hierarchy. When `--full` is active:
1. `ripdoc` identifies the target's `Id` and associated `impl` IDs.
2. The physical `.rs` file is read and sliced using the line/column `Span` metadata.
3. Raw text is injected into the rendering stream, maintaining correct indentation within the module block.
