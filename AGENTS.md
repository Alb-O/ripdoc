# PRODUCTIVITY: Skim any crate APIs/docs through simple cli

- Usage: `ripdoc <COMMAND> [TARGET] [OPTIONS]`
- Basic skim from any directory: `ripdoc print [target]` → printed Markdown with top-level docs and code skeleton
- Can target crates.io (no url needed, just name) or local crate paths
- Get structure only: `ripdoc list [target] [--search/-s term]` → list of modules/macros/types with source locations
- Search within docs: `ripdoc print [target] --search <term>` (combine with `--search-spec name,doc,path,signature` and `--direct-match-only`/`-d` to avoid auto-expanding parents)
- OR searches: `ripdoc print gix --search "init|clone|fetch|remote|config"`
- Raw data for tooling: `ripdoc raw [target]` → JSON rustdoc model; useful with `jq`/scripts
- Fetch README: `ripdoc readme [target]` → fetches and displays the README
- Private/auto trait details: add `--private` and/or `--auto-impls`
- Feature gating flags work the same as with cargo