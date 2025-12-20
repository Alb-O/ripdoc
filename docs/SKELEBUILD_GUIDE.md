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

# Or: add literal file content for the containing file
# ripdoc skelebuild add bat::config::Config --raw-source

# Insert your own notes (prefer target-relative insertion)
ripdoc skelebuild inject '## Notes\nWhy this matters...' --after-target bat::config::Config

# Show indices / full state when needed
ripdoc skelebuild status

# Remove entries by exact target/content
ripdoc skelebuild remove bat::assets::get_acknowledgements

# Regenerate the output using the current entry list
ripdoc skelebuild rebuild
```

## Tips

- Prefer `inject --after-target <spec>`; `--at <index>` works but indices shift as you insert.
- `status` is read-only (it won’t rewrite your output file); pass `--show-state` to print the full state after other commands.
- `--implementation` includes method/function bodies when available; non-callables still render as a skeleton for context.
- If a target can’t be resolved or a source file can’t be read, rebuild writes a visible Markdown warning block (`> [!ERROR] ...`) so missing code isn’t silent.

## Finding the right item path

For local targets, the `crate::...` prefix comes from rustdoc.

- For bin crates, that prefix is often the *bin name*, not the folder/package name.
- You can usually omit the crate prefix and use a suffix path (e.g. `terminal_panel::TerminalState`).
- If you’re not sure, discover paths first:

```bash
ripdoc list ./path/to/crate --search TerminalState --search-spec path
```

`skelebuild` tries some path fallbacks (e.g. stripping/replacing a mismatched crate prefix) and prefers matches whose source lives in the target crate. For maximum precision, use the exact `crate::...` path from `list`.

## Positional item mode

Both of these work:

- `ripdoc skelebuild add ./path/to/crate crate::module::Type`
- `ripdoc print ./path/to/crate crate::module::Type`
