# Ripdoc

- Usage: `ripdoc <COMMAND> [TARGET] [OPTIONS]`
- Targets: crates.io names (`serde`), `name@version`, or local paths (`./path/to/crate`).

## Commands

- `ripdoc print` - render items as Markdown (see `ripdoc agents print`)
- `ripdoc list` - list items with source locations
- `ripdoc skelebuild` - stateful context builder (see `ripdoc agents skelebuild`)
- `ripdoc raw` - output raw rustdoc JSON

## Quick Examples

```bash
# Print a crate or specific item
ripdoc print serde
ripdoc print serde::Deserialize

# Search within a crate
ripdoc print tokio --search "spawn"

# Discover exact paths
ripdoc list serde --search "Deserialize" --search-spec path

# Build context incrementally
ripdoc skelebuild add ./my-crate crate::Config
```

## Common Options

- `--search <query>` - filter by regex pattern
- `--search-spec name,doc,signature,path` - search domains
- `--implementation` - include method bodies
- `--raw-source` - include full source files
- `--private` - include private items
- `--features <list>` - enable crate features

## Topic Guides

- `ripdoc agents print` - detailed print command usage
- `ripdoc agents skelebuild` - stateful context building
