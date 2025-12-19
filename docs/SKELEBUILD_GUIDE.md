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
When you need to see the internal implementation of a specific component, use the `--full` (or `-f`) flag on `skelebuild add`. `ripdoc` will extract the actual source code and associated `impl` blocks from the disk.

> `--full` is a `skelebuild add` flag (not a `ripdoc print` flag).

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
Use the `inject` subcommand to add your own analysis or notes between code sections.

- Prefer `--at <index>` for reliability (use `ripdoc skelebuild status` to see indices).
- Use `--after <string>` for quick placement; it matches by exact string or prefix, and errors if there are 0 or multiple matches.

> **Shell Quoting**: Use **single quotes** (`'...'`) for injection text containing Markdown or code references like `min()` or `max()`. Double quotes cause Bash to interpret parentheses as command substitution, leading to errors like `command not found`.

```bash
# Show indices
ripdoc skelebuild status

# Robust: insert at a specific index
ripdoc skelebuild inject '## Analysis: Paging Logic
The `run()` method handles the core paging state machine using min() and max().' --at 3

# Convenient: insert after a previous target (exact or prefix match)
ripdoc skelebuild inject '## Analysis: Paging Logic
The `run()` method handles the core paging state machine.' --after bat::controller::Controller::run

# Incorrect: double quotes cause shell errors
# ripdoc skelebuild inject "calls min() and max()" ...
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

# Rebuild the output without adding new targets (useful after code changes)
ripdoc skelebuild rebuild

# Remove a specific entry by its target path or injection content
ripdoc skelebuild remove bat::assets::get_acknowledgements

# Reset clears entries but PRESERVES output path and --flat setting
ripdoc skelebuild reset
```

> **Note**: `reset` clears all entries but preserves your `--output` and `--flat` configuration from the previous session. To fully reset everything, delete the state file at `~/.local/state/ripdoc/skelebuild.json`.

## Features

- **Flattening (`--flat`)**: Skips the recursive `pub mod` wrapping for a more readable, highlight-oriented document.
- **Lazy Flagging**: Flags like `--output`, `--format`, and `--flat` can be placed anywhere in the command string.
- **Automatic Impl Tracking**: Adding a struct or enum with `--full` automatically marks its associated `impl` blocks for injection.
- **Manual Injection**: Interleave your own Markdown commentary into the stateful build without it being overwritten.
- **Item Deduplication**: Items are only rendered once; re-exports and glob imports skip already-visited items to avoid redundancy.
- **Shared Visited Set**: Across all targets in a single build, a global visited set ensures items reachable through multiple paths (e.g., prelude re-exports) appear exactly once.

## Technical Summary

`skelebuild` leverages `rustdoc` JSON to resolve item spans and `SearchIndex` to preserve (or flatten) hierarchy. When `--full` is active:
1. `ripdoc` identifies the target's `Id` and associated `impl` IDs.
2. The physical `.rs` file is read (using workspace-aware path heuristics).
3. The source is sliced using line/column `Span` metadata.
4. Raw text or manual commentary is injected into the rendering stream in the order specified by the state.

### Deduplication

A **shared visited set** (`Arc<Mutex<HashSet<Id>>>`) spans all renderer instances in a single build. This ensures:
- Items reached through multiple paths (e.g., a struct defined in a private module and re-exported publicly) are only rendered once.
- Visibility is checked **before** descending into containers, so items inside private modules aren't prematurely marked as visited.
- When an `impl` block is matched, its target struct and ancestors are automatically added to the render context to prevent orphaned impl blocks in `--flat` mode.

### Intelligent Elision with `--full`

Even with `--full`, ripdoc performs **intelligent slicing** rather than raw file concatenation:
- The full implementation of the **target item** (struct, function, impl block) is extracted.
- Unrelated module-level code (imports, sibling items, private helpers) is elided with `// ...` markers.
- This keeps the skeleton focused on the requested API surface while preserving implementation details where they matter.

If you need the complete, unmodified source of a file, use `ripdoc print <crate>::<module> --full` directly instead of skelebuild.

