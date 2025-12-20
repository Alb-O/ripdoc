# Ripdoc `skelebuild`

`skelebuild` incrementally builds a Markdown “source map” by mixing API skeletons, selective implementation spans, and your own commentary. State is persisted at `~/.local/state/ripdoc/skelebuild.json`.

## Minimal workflow

```bash
# Start fresh (preserves --output/--plain from last time unless overridden)
ripdoc skelebuild reset --output bat_map.md --plain

# Add an item (outline)
ripdoc skelebuild add bat::config::Config

# Add an item with its implementation span (reads from disk)
ripdoc skelebuild add bat::controller::Controller::run --implementation

# Add multiple related items at once (same crate target prefix)
ripdoc skelebuild add ./tome/bin/tome-term \
  tome::editor::Editor::render \
  tome::editor::Editor::ensure_cursor_visible \
  tome::editor::Editor::scroll_down_one_visual_row \
  --implementation

# Add raw source directly from disk (useful for tests / code not in rustdoc)
ripdoc skelebuild add-raw ./path/to/file.rs:336:364

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
```

## Tips

- Prefer `inject --after-target <spec>` / `--before-target <spec>`; `--at <index>` works, but indices shift as you insert.
- Most commands print a single summary line; `status` is read-only (it won’t rewrite your output file) and is the easiest way to see indices; pass `--show-state` to print the full state after other commands.
- `preview` prints the fully rebuilt Markdown to stdout without writing the output file.
- `--implementation` includes method/function bodies when available; for containers it will also pull in relevant `impl` blocks when possible.
- Impl-block targeting: you can target an entire impl block with `Type::Trait` (e.g. `Editor::EditorOps`).
- Raw source snippets: `skelebuild add-raw /path/to/file.rs:START:END` injects arbitrary line ranges (useful for tests which may not appear in rustdoc JSON).
- Validation: `skelebuild add` validates targets by default and fails early; pass `--no-validate` to record an entry without validating it.
- Errors/warnings are printed to stderr and are not embedded into the generated Markdown output (keeps the output doc clean).
- Source path resolution is crate-root aware: relative spans like `src/main.rs` are resolved against the target crate, not your current working directory.
- Markdown interleaving: `skelebuild` inserts blank lines between blocks, but if you inject an unterminated list/callout, add a trailing blank line so the next `### Source: ...` header doesn’t get “captured” by Markdown formatting.
- Fix typos / toggle flags: use `skelebuild update <spec> [--implementation|--no-implementation] [--raw-source|--no-raw-source]`.
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
- If you’re not sure, discover paths first:

```bash
ripdoc list ./path/to/crate --search TerminalState --search-spec path --private
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
