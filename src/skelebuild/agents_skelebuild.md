# Ripdoc `skelebuild`

`skelebuild` incrementally builds a Markdown "source map" by mixing API skeletons, selective implementation spans, and your own commentary. State is persisted at `~/.local/state/ripdoc/skelebuild.json`.

## Workflow

```bash
# Start fresh (preserves --output/--plain from last time unless overridden)
ripdoc skelebuild reset --output bat_map.md --plain

# Add an item (outline only)
ripdoc skelebuild add bat::config::Config

# Add with implementation span
ripdoc skelebuild add bat::controller::Controller::run --implementation

# Add a private item (not in public API)
ripdoc skelebuild add bat::internal::Parser --private --implementation

# Add multiple items at once
ripdoc skelebuild add ./tome/bin/tome-term \
  tome::editor::Editor::render \
  tome::editor::Editor::ensure_cursor_visible \
  --implementation

# Add raw source directly from disk (for tests / code not in rustdoc)
ripdoc skelebuild add-raw ./path/to/file.rs:336:364
ripdoc skelebuild add-file ./path/to/file.rs  # entire file

# Add context from git diffs
ripdoc skelebuild add-changed --git HEAD^..HEAD --only-rust
ripdoc skelebuild add-changed --staged --only-rust

# Insert notes (prefer target-relative insertion; `\n` becomes newline)
ripdoc skelebuild inject '## Notes\nWhy this matters...' --after-target bat::config::Config

# Inject from stdin to avoid shell-escaping issues
ripdoc skelebuild inject --from-stdin --after-target bat::config::Config <<'EOF'
## Notes
My commentary with 'quotes' and $variables
EOF

# Other commands
ripdoc skelebuild preview      # print output without writing file
ripdoc skelebuild status       # show entries and indices
ripdoc skelebuild update bat::config::Config --implementation
ripdoc skelebuild remove bat::assets::get_acknowledgements
ripdoc skelebuild --plain      # toggle plain mode (auto-rebuilds)
```

## Tips

- **Injection placement**: Prefer `--after-target <spec>` / `--before-target <spec>` over `--at <index>` (indices shift as you insert).
- **`--implementation`**: Includes function/method bodies; for structs/enums also pulls in local `impl` blocks.
- **`--private`**: Enables resolving private items not in the public API.
- **Impl-block targeting**: Target an entire impl with `Type::Trait` (e.g. `Editor::EditorOps`).
- **Raw source**: Use `add-raw path:START:END` or `add-file path` for code not in rustdoc (tests, macros, generated code).
- **Validation**: `add` validates by default; use `--no-validate` to skip.
- **Inject escaping**: `\n`, `\t`, `\\` are unescaped by default; use `--literal` to keep backslashes.
- **Errors to stderr**: Warnings/errors go to stderr, keeping the output doc clean.

## Target Resolution

Target specs can be paths (`./path/to/crate`) or names (`serde`). Both forms work:

```bash
ripdoc skelebuild add ./path/to/crate crate::module::Type
ripdoc skelebuild add serde::de::Deserialize
```

**Resolution rules:**
- From inside a Cargo workspace: resolves workspace members and dependencies by name.
- Outside a workspace: tries crates.io / local cache.
- For bin crates, the `crate::` prefix is often the *bin name*, not the package name.
- You can usually omit the crate prefix (e.g. `terminal_panel::TerminalState`).

**Finding paths:** Use `ripdoc list` to discover exact paths:

```bash
ripdoc list ./path/to/crate --search TerminalState --search-spec path --private
```

**Inherent vs trait methods:** If ambiguous, use a more specific path:
- Inherent: `crate::Type::method`
- Trait: `crate::Trait::method`
- Fully-qualified: `<crate::Type as crate::Trait>::method`

## Speed Guide

### Core Pattern: Parallel Bash + Sequential `&&` Chains

Execute multiple **parallel bash calls**, each containing a **sequential `&&` chain** of related operations.

### Turbo Execution Pattern

```json
{
  "recipient_name": "multi_tool_use.parallel",
  "parameters": {
    "tool_uses": [
      {
        "recipient_name": "functions.bash",
        "parameters": {
          "command": "ripdoc skelebuild add ./crate Path1::Item1 --implementation && ripdoc skelebuild add ./crate Path1::Item2 --implementation",
          "description": "Trace code path A"
        }
      },
      {
        "recipient_name": "functions.bash",
        "parameters": {
          "command": "ripdoc skelebuild add ./crate Path2::Item1 --implementation && ripdoc skelebuild add ./crate Path2::Item2 --implementation",
          "description": "Trace code path B"
        }
      }
    ]
  }
}
```

### Strategy for 1000+ Line Context in <60s

1. **Initialize:** `ripdoc skelebuild reset --output <file>.md --plain`
2. **Parallel blast:** Launch 5-10 parallel bash tools covering:
   - Core structs & lifecycle methods
   - Primary API implementation
   - Error handling paths
   - Tests & raw source (`add-raw`)
3. **Read output:** The file auto-updates after each command.

### Rules of Thumb

- **Sequential (`&&`)**: Within a single code path where order matters.
- **Parallel**: For unrelated paths or different subsystems.
- **Always use `--implementation`** for methods to get full source context.

## Troubleshooting: Empty Output

If skelebuild warns that entries exist but output is nearly empty:

```
Warning: 5 target entries exist but rebuilt output is nearly empty (0 chars).
```

**Causes & solutions:**

1. **Private items**: Use `--private` flag when adding.
2. **Feature-gated code**: Use `--features my_feature` when adding.
3. **Code not in rustdoc**: Use `add-raw` or `add-file` instead.
4. **Wrong paths**: Discover exact path with `ripdoc list --search <name> --private`.
