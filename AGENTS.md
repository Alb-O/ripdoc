# PRODUCTIVITY: Skim any crate APIs/docs through simple cli

- Usage: `ripdoc [OPTIONS] [COMMAND] [TARGET]`
- Basic skim from any directory: `ripdoc print [target]` → printed Markdown with top-level docs and code skeleton
- Can target crates.io (no url needed, just name) or local crate paths
- Get structure only: `ripdoc list <target> [--search/-s term]` → list of modules/macros/types with source locations
- Search within docs: `ripdoc print <target> --search <term>` (combine with `--search-spec name,doc,path,signature` and `--direct-match-only`/`-d` to avoid auto-expanding parents)
- Raw data for tooling: `ripdoc raw <target>` → JSON rustdoc model; useful with `jq`/scripts
- Private/auto trait details: add `--private` and/or `--auto-impls`
- Feature gating flags work the same as with cargo
