# Ripdoc `skelebuild`

`skelebuild` incrementally builds a Markdown "source map" by mixing API skeletons, selective implementation spans, and your own commentary. State is persisted at `~/.local/state/ripdoc/skelebuild.json`.

## Minimal workflow

```bash
# Start fresh (preserves --output/--plain from last time unless overridden)
ripdoc skelebuild reset --output bat_map.md --plain

# Add an item (outline)
ripdoc skelebuild add bat::config::Config

# Add an item with its implementation span (reads from disk)
ripdoc skelebuild add bat::controller::Controller::run --implementation

# Add a private item (not exported in public API)
ripdoc skelebuild add bat::internal::Parser --private --implementation

# Add multiple related items at once (same crate target prefix)
ripdoc skelebuild add ./tome/bin/tome-term \
  tome::editor::Editor::render \
  tome::editor::Editor::ensure_cursor_visible \
  tome::editor::Editor::scroll_down_one_visual_row \
  --implementation

# Add raw source directly from disk (useful for tests / code not in rustdoc)
ripdoc skelebuild add-raw ./path/to/file.rs:336:364

# Add an entire file as raw source
ripdoc skelebuild add-file ./path/to/file.rs

# Add "changed context" from git diffs (default: tries to resolve touched rustdoc items,
# and also includes raw-source snippets around each diff hunk)

# Last commit only (the commit at HEAD vs its parent):
ripdoc skelebuild add-changed --git HEAD^..HEAD --only-rust

# Working tree diffs:
ripdoc skelebuild add-changed --only-rust              # unstaged changes
ripdoc skelebuild add-changed --staged --only-rust     # staged changes
ripdoc skelebuild add-changed --git main...HEAD --only-rust

# Insert your own notes (prefer target-relative insertion)
# `inject` interprets `\n` as newlines by default.
ripdoc skelebuild inject '## Notes\nWhy this matters...' --after-target bat::config::Config

# Avoid shell-escaping headaches: inject from stdin or a file
ripdoc skelebuild inject --from-stdin --after-target bat::config::Config <<'EOF'
## Notes
My commentary with 'quotes' and $variables
EOF

# Preview the rebuilt output without writing the file
ripdoc skelebuild preview

# Show indices / full state when needed
ripdoc skelebuild status

# Toggle flags on an existing entry
ripdoc skelebuild update bat::config::Config --implementation

# Remove entries by exact target/content
ripdoc skelebuild remove bat::assets::get_acknowledgements

# Regenerate the output using the current entry list
ripdoc skelebuild rebuild

# Toggle plain mode on or off (triggers automatic rebuild)
ripdoc skelebuild --plain      # enable plain output
ripdoc skelebuild --no-plain   # disable plain output (use module nesting)
```

## Tips

- Prefer `inject --after-target <spec>` / `--before-target <spec>`; `--at <index>` works, but indices shift as you insert.
- Most commands print a single summary line; `status` is read-only (it won't rewrite your output file) and is the easiest way to see indices; pass `--show-state` to print the full state after other commands.
- `preview` prints the fully rebuilt Markdown to stdout without writing the output file.
- `--implementation` includes method/function bodies when available; for containers it will also pull in relevant `impl` blocks when possible.
- `--private` enables searching for private items when resolving targets (useful for internal modules, private methods, etc.).
- Impl-block targeting: you can target an entire impl block with `Type::Trait` (e.g. `Editor::EditorOps`).
- Raw source snippets: `skelebuild add-raw /path/to/file.rs:START:END` injects arbitrary line ranges (useful for tests which may not appear in rustdoc JSON). Use `skelebuild add-file /path/to/file.rs` to include a whole file.
- Validation: `skelebuild add` validates targets by default and fails early; pass `--no-validate` to record an entry without validating it.
- Errors/warnings are printed to stderr and are not embedded into the generated Markdown output (keeps the output doc clean).
- **Empty output warning**: If entries exist but the rebuilt output is nearly empty, skelebuild warns you with suggested remedies (use `add-raw`/`add-file`, enable features, check paths).
- Source path resolution is crate-root aware: relative spans like `src/main.rs` are resolved against the target crate, not your current working directory.
- Markdown interleaving: `skelebuild` inserts blank lines between blocks, but if you inject an unterminated list/callout, add a trailing blank line so the next `### Source: ...` header doesn't get "captured" by Markdown formatting.
- Fix typos / toggle flags: use `skelebuild update <spec> [--implementation|--no-implementation] [--raw-source|--no-raw-source]`.
- Toggle plain mode: use `ripdoc skelebuild --plain` or `--no-plain` to switch output modes; this triggers an automatic rebuild if entries exist.
- Inject quoting: `inject` unescapes `\n`, `\t`, and `\\` by default; pass `--literal` to keep backslashes (useful for regexes or showing escape sequences).
- Inject content sources: pass `inject --from-stdin` (heredocs) or `inject --from-file <path>`.

## Local crates vs crates.io

When you pass a bare crate name (e.g. `serde` or `tome_core`), ripdoc treats it as a *named target*. Resolution depends on where you run it:

- From inside a Cargo workspace/package: ripdoc can resolve workspace members and dependencies by name.
- Outside any Cargo workspace/package: ripdoc will try crates.io / local cache.

If you meant a local crate and you’re not running from its workspace root, pass a filesystem path as the first argument:

```bash
ripdoc skelebuild add ./path/to/crate crate::module::Item
```

## Finding the right item path

For local targets, the `crate::...` prefix comes from rustdoc.

- For bin crates, that prefix is often the *bin name*, not the folder/package name.
- You can usually omit the crate prefix and use a suffix path (e.g. `terminal_panel::TerminalState`).
- Private items are excluded from `ripdoc list` by default; pass `--private` when searching for private methods.
- **To add private items to skelebuild**, use the `--private` flag: `ripdoc skelebuild add <target> --private`
- If you're not sure, discover paths first:

```bash
ripdoc list ./path/to/crate --search TerminalState --search-spec path --private
```

Then add with the `--private` flag if the item is private:

```bash
ripdoc skelebuild add ./path/to/crate::internal::TerminalState --private --implementation
```

### Inherent vs trait methods

If a type has both an inherent method and a trait method with the same name, you may need to use a more specific path:

- Inherent method: `crate::Type::method`
- Trait method (often): `crate::Trait::method`
- Fully-qualified (when needed): `<crate::Type as crate::Trait>::method`

`skelebuild` tries some path fallbacks (e.g. stripping/replacing a mismatched crate prefix) and prefers matches whose source lives in the target crate. For maximum precision and disambiguation, use the exact path from `ripdoc list ... --search <name> --search-spec path`.

## Positional item mode

Both of these work:

- `ripdoc skelebuild add ./path/to/crate crate::module::Type`
- `ripdoc print ./path/to/crate crate::module::Type`

## Speed Guide

### Core Pattern: Parallel Bash Instances + `&&` Chains

To build context at maximum speed, execute multiple **parallel bash tool calls** simultaneously, each containing a **sequential `&&` chain** of related operations.

### Turbo Execution Pattern

Trace multiple code paths simultaneously by launching parallel bash tools in a single response:

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

1.  **Initialize once:** `ripdoc skelebuild reset --output <filename>.md --plain`
2.  **Parallel Blast:** Launch 5-10 parallel `bash` tools.
    *   **Track 1:** Core structs & lifecycle methods.
    *   **Track 2:** Primary API surface implementation.
    *   **Track 3:** Error handling & recovery paths.
    *   **Track 4:** Test cases & raw source critical sections (`add-raw`).
3.  **Finalize once:** `ripdoc skelebuild rebuild`

### Rules of Thumb

*   **Sequential (`&&`)**: Use within a single code path where order matters.
*   **Parallel**: Use for unrelated code paths or different subsystems.
*   **Batching**: Always use `--implementation` for methods to get full source context.
*   **Deferred Rebuild**: Never rebuild until the very end to avoid redundant I/O.

## Troubleshooting

### Empty or nearly empty output

If `skelebuild` warns that entries exist but output is nearly empty:

```
Warning: 5 target entries exist but rebuilt output is nearly empty (0 chars).
This may indicate that the targets could not be resolved. Common causes:
  - Private items not visible in rustdoc output
  - Feature-gated modules not enabled
  - Incorrect module paths
```

**Solutions:**

1. **Private items**: Add the `--private` flag when adding targets:
   ```bash
   ripdoc skelebuild add ./crate::internal::Item --private --implementation
   ```

2. **Feature-gated code**: Enable features when building rustdoc:
   ```bash
   ripdoc skelebuild add ./crate::feature_mod::Item --features my_feature
   ```

3. **Code not in rustdoc**: Use raw source for tests, macros, or generated code:
   ```bash
   ripdoc skelebuild add-raw ./path/to/file.rs:100:150
   ripdoc skelebuild add-file ./path/to/tests.rs
   ```

4. **Incorrect paths**: Discover the exact path first:
   ```bash
   ripdoc list ./crate --search ItemName --search-spec path --private
   ```

### Switching between plain and nested output

Use `--plain` or `--no-plain` to toggle output modes. This automatically triggers a rebuild:

```bash
# Enable plain output (flat, no module nesting)
ripdoc skelebuild --plain

# Disable plain output (hierarchical module structure)
ripdoc skelebuild --no-plain
```
