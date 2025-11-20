# PRODUCTIVITY: Skim any crate APIs/docs through simple cli

- Usage: `ripdoc [GLOBAL_FLAGS] [TARGET] [SUBCOMMAND ...]`
- Basic skim from any directory: `ripdoc [target]` or `ripdoc render [target]` → rendered Markdown with top-level docs and code skeleton (use `--format rust` for Rust-like signatures)
- Can target crates.io (no url needed, just name) or local crate paths
- Get structure only: `ripdoc list <target> [--search term]` → list of modules/macros/types with source locations
- Search within docs: `ripdoc search <target> <term>` (combine with `--search-spec name,doc,path,signature` and `--direct-match-only`/`-d` to avoid auto-expanding parents)
- One-arg search fallback: `ripdoc search <term>` (no target) runs `cargo search <term>`
- Raw data for tooling: `ripdoc raw <target>` → JSON rustdoc model; useful with `jq`/scripts
- Private/auto trait details: add `--private` and/or `--auto-impls`
- Feature gating flags work the same as with cargo
