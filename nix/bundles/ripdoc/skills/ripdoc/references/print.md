# print

`print` renders rustdoc items as Markdown with optional source code. Use it for one-shot queries or piping into other tools.

## Basic Usage

```bash
# Print full crate skeleton
ripdoc print serde

# Print specific item by path
ripdoc print serde serde::Deserialize
ripdoc print ./path/to/crate crate::module::Type

# Compact form (target::path)
ripdoc print serde::Deserialize
ripdoc print ./path/to/crate::crate::module::Type
```

## Search Mode

Search across items without knowing exact paths:

```bash
# Basic search (searches names by default)
ripdoc print serde --search "Deserialize"

# Search in specific domains
ripdoc print serde --search "parse" --search-spec name,signature
ripdoc print serde --search "lifetime" --search-spec doc

# OR queries (regex)
ripdoc print tokio --search "spawn|block_on|runtime"

# Case-sensitive search
ripdoc print serde --search "JSON" --case-sensitive
```

**Search domains** (`--search-spec`):
- `name` - item names (default)
- `path` - full `crate::module::Item` paths
- `doc` - documentation text
- `signature` - function/type signatures

## Output Control

```bash
# Include implementation spans (method bodies, not just signatures)
ripdoc print serde::Deserialize --implementation

# Include raw source files
ripdoc print ./my-crate crate::Config --raw-source

# Disable source location labels
ripdoc print serde --no-source-labels

# Force no color (also: NO_COLOR=1 env var)
ripdoc print serde --no-color
```

## Implementation Mode (`--implementation`)

By default, `print` shows signatures only. Use `--implementation` to include:
- Function/method bodies
- Full `impl` blocks for types
- Macro definitions

This is useful when you need to understand *how* something works, not just its API.

## Raw Source Mode (`--raw-source`)

Includes the entire source file for matched items. Useful when:
- You need surrounding context
- Code isn't fully captured by rustdoc
- You want to see imports/modules structure

## Path Resolution

Ripdoc uses rustdoc paths, which may differ from Rust's `use` paths:

```bash
# Discover exact paths with `list`
ripdoc list serde --search "Deserialize" --search-spec path

# For bin crates, the crate name is often the binary name
ripdoc print ./my-bin binname::main
```

**Tips:**
- Use `crate::` prefix for local crates
- For re-exports, the path is where the item is *defined*, not re-exported
- Use `--private` to include private items in resolution

## Feature Flags

```bash
# Enable specific features
ripdoc print tokio --features "full,rt-multi-thread"

# Enable all features
ripdoc print tokio --all-features

# Disable default features
ripdoc print tokio --no-default-features
```

## Combining with Other Tools

```bash
# Pipe to less/bat for paging
ripdoc print serde | bat -l md

# Extract to file
ripdoc print serde::Serialize > serialize.md

# Use with grep for quick filtering
ripdoc print tokio --search "spawn" | grep -A5 "^##"
```

## Troubleshooting

**"No matches found":**
1. Check the exact path: `ripdoc list <target> --search "<name>" --search-spec path --private`
2. The item might be re-exported; search by name to find the definition path
3. For private items, ensure you're using `--private`

**Missing items:**
- Feature-gated items need `--features` flag
- Private items need `--private` flag
- Proc-macro crates have limited rustdoc output

**Path confusion:**
- Bin crates use the binary name as crate root, not the package name
- Re-exports appear at their definition site, not the re-export location
