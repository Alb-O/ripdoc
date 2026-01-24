---
name: ripdoc
description: Query Rust docs and crate APIs from CLI. Use when exploring Rust crates, building API context and codemaps for handoffs, searching rustdoc, or needing crate skeletons. Trigger on mention of ripdoc, rust docs, crate API, skelebuild, codemap, or when investigating Rust library usage.
---

# Ripdoc

Query Rust docs and crate API from the command line. For AI agents, ripdoc provides on-demand access to crate documentation from any source (local filesystem or crates.io).

## Quick Reference

```bash
# Print a crate or specific item
ripdoc print serde
ripdoc print serde::Deserialize

# Search within a crate
ripdoc print tokio --search "spawn"

# Discover exact paths
ripdoc list serde --search "Deserialize" --search-spec path

# Build context incrementally (skelebuild)
ripdoc skelebuild reset --output context.md
ripdoc skelebuild add ./my-crate crate::Config
```

## Commands

- `ripdoc print` - Render items as Markdown
- `ripdoc list` - List items with source locations
- `ripdoc skelebuild` - Stateful context builder for codemaps
- `ripdoc readme` - Print crate README

## Common Options

- `--search <query>` - Filter by regex pattern
- `--search-spec name,doc,signature,path` - Search domains
- `--implementation` - Include method bodies
- `--raw-source` - Include full source files
- `--private` - Include private items
- `--features <list>` - Enable crate features

## References

Read for more info when needed:

- [Print command details](references/print.md)
- [Skelebuild for building codemaps](references/skelebuild.md)
