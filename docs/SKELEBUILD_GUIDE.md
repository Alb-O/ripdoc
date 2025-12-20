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

# Insert your own notes (use status to find indices)
ripdoc skelebuild status
ripdoc skelebuild inject '## Notes\nWhy this matters...' --at 1

# Remove entries by exact target/content
ripdoc skelebuild remove bat::assets::get_acknowledgements

# Regenerate the output using the current entry list
ripdoc skelebuild rebuild
```

## Tips

- Prefer `inject --at <index>`; `--after <prefix>` is convenience-only and can be ambiguous.
- `status` is read-only (it won’t rewrite your output file).
- If a target can’t be resolved or a source file can’t be read, rebuild writes a visible Markdown warning block (`> [!ERROR] ...`) so missing code isn’t silent.

## Positional item mode

Both of these work:

- `ripdoc skelebuild add ./path/to/crate crate::module::Type`
- `ripdoc print ./path/to/crate crate::module::Type`
