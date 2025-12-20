# Ripdoc

- Usage: `ripdoc <COMMAND> [TARGET] [OPTIONS]`
- Targets: crates.io names (`serde`), `name@version`, or local paths (`./path/to/crate`).

## Common flows

- Print skeleton: `ripdoc print [target]`
- Print a specific item (path mode): `ripdoc print [target] [ITEM]`
  - Also supported: `ripdoc print ./path/to/crate::crate::Type`
- Search: `ripdoc print [target] --search <query>`
  - Domains: `--search-spec name,doc,signature,path`
  - OR queries: `--search "init|clone|fetch"`
- List items: `ripdoc list [target] [--search <query>]` (includes source locations)
  - Tip: use `--search-spec path` to discover the exact `crate::...` path ripdoc expects.
  - Note: for local *bin* crates, the rustdoc crate name is often the bin name (e.g. `binname::...`), not the folder/package name. `skelebuild` can usually match the suffix without the crate prefix.
- Raw rustdoc JSON: `ripdoc raw [target]`

## Output knobs

- Source labels: `--no-source-labels`
- Include implementation spans: `--implementation` (for containers, may include whole `impl` blocks for best detail)
- Include whole files: `--raw-source` / `--source`
- Color: auto-disabled when stdout isn’t a TTY; force off with `--no-color` or `NO_COLOR=1`.

## skelebuild

- Stateful builder: `ripdoc skelebuild add <target> [ITEM] [--implementation|--raw-source]`
  - For local projects, use a path target: `ripdoc skelebuild add ./path/to/crate crate::Item`.
- For more detailed usage of `skelebuild`, see `ripdoc skelebuild agents`
