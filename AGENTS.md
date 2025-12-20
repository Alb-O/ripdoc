# ripdoc quick notes

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
  - Validates by default; pass `--no-validate` to skip validation.
- Update flags: `ripdoc skelebuild update <spec> [--implementation|--no-implementation] [--raw-source|--no-raw-source]`
- Impl-block targeting: `ripdoc skelebuild add ./path/to/crate Type::Trait`
- Interleave notes: `ripdoc skelebuild inject '...markdown...' --after-target <spec>` (or `--before-target`; `--at <index>` is precise but indices shift)
  - `inject` unescapes `\n` by default; pass `--literal` to keep backslashes.
- Markdown spacing: if you inject lists/callouts, end with a blank line so the next `### Source: ...` header stays readable.
- Source spans: ripdoc resolves relative span paths against the target crate root.
- Manage state: `status` (read-only), `rebuild` (rewrites output), `remove`, `reset` (use `skelebuild --show-state ...` to print full state)
- State file: `~/.local/state/ripdoc/skelebuild.json`
