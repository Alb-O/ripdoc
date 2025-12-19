# Ripdoc Skelebuild: Usage Guide

`skelebuild` allows you to incrementally construct tailored "source maps" of Rust crates by mixing high-level API signatures with full source implementations. It maintains state in the XDG state directory, enabling multi-session construction.

## Step-by-Step Example: Mapping the `bat` Crate

### 1. Initialize with Structural Outlines
Start by defining your output file. Use the `--flat` flag if you want to skip the `pub mod` hierarchy and produce a cleaner, flatter document.

```bash
ripdoc skelebuild reset
# Note: Flags like --output can be placed anywhere in the command
ripdoc skelebuild add bat::config::Config --flat --output bat_map.md
```

**Expected Output (`bat_map.md`):**
```rust
### Source: src/config.rs

pub struct Config<'a> {
    pub language: Option<&'a str>,
    pub colored_output: bool,
    // ... (other fields)
}
```

### 2. Inject Core Logic
When you need to see the internal implementation of a specific component, use the `--full` (or `-f`) flag. `ripdoc` will extract the actual source code and associated `impl` blocks from the disk.

```bash
ripdoc skelebuild add bat::controller::Controller::run --full
```

**Updated Output (`bat_map.md`):**
Note how the implementation is injected directly after a file header, without deep `pub mod` nesting.

```rust
### Source: src/config.rs
pub struct Config<'a> { ... }

### Source: src/controller.rs

impl Controller<'_> {
    pub fn run(&self, inputs: Vec<Input>) -> Result<bool> {
        // actual implementation code...
        self.run_with_error_handler(inputs, output_handle, default_error_handler)
    }
}
```

### 3. Interleave Manual Commentary
Use the `inject` subcommand to add your own analysis or notes between code sections. Use `--after <target>` to place the comment precisely.

```bash
ripdoc skelebuild inject "## Analysis: Paging Logic
The following method handles the core paging state machine." --after bat::controller::Controller::run
```

**Updated Output (`bat_map.md`):**
```rust
### Source: src/controller.rs
impl Controller<'_> { ... }

## Analysis: Paging Logic
The following method handles the core paging state machine.
```

## State Management

Review tracked entries or prune them as the skeleton evolves. State persists until `reset`.

```bash
# Check current entries (targets and injections)
ripdoc skelebuild status

# Remove a specific entry by its target path or injection content
ripdoc skelebuild remove bat::assets::get_acknowledgements
```

## Features

- **Flattening (`--flat`)**: Skips the recursive `pub mod` wrapping for a more readable, highlight-oriented document.
- **Lazy Flagging**: Flags like `--output`, `--format`, and `--flat` can be placed anywhere in the command string.
- **Automatic Impl Tracking**: Adding a struct or enum with `--full` automatically marks its associated `impl` blocks for injection.
- **Manual Injection**: Interleave your own Markdown commentary into the stateful build without it being overwritten.

## Technical Summary

`skelebuild` leverages `rustdoc` JSON to resolve item spans and `SearchIndex` to preserve (or flatten) hierarchy. When `--full` is active:
1. `ripdoc` identifies the target's `Id` and associated `impl` IDs.
2. The physical `.rs` file is read (using workspace-aware path heuristics).
3. The source is sliced using line/column `Span` metadata.
4. Raw text or manual commentary is injected into the rendering stream in the order specified by the state.

