# Ripdoc

Ripdoc prints a syntactical outline of a crate's public API and documentation. The CLI provides on-demand access to these resources from any source (local filesystem or through [crates.io](https://crates.io)), perfect for AI agent usage.

## For AI Agents

Ripdoc is built to be the "eyes" of an AI agent in a Rust codebase.

- `print` and `list` subcommands explores and prints crate API (local or from crates.io), searches/filters, and provides agent-consumable markdown-formatted context.
- `readme` subcommand prints just the contents of a crate's README.
- `skelebuild` is a statefule, semi-interactive mode for constructing "source maps" for local crates that you or your agents are developing. The tool builds an output markdown file intelligently, inserting slices of contextual rust code in a correct, syntax-aware order and nesting. Agents can interleave API skeletons, implementation spans, and general markdown commentary to explain code and architecture for the next agent in the loop (or the user, if you care enough).

For dense technical guides specifically for agent consumption, see:
- `.agents/skills/ripdoc/SKILL.md` (general usage)
- `.agents/skills/ripdoc/references/print.md` (`print` command details)
- `.agents/skills/ripdoc/references/skelebuild.md` (`skelebuild` workflow)

There is no MCP server for this tool. Long live skills!

## Skelebuild ("Hand-off" Mode)

`skelebuild` is a stateful workflow for constructing a "source map" markdown file that interleaves:

- Rendered API skeletons (and optionally implementation spans)
- Full raw source for specific files when needed
- Commentary/instructions between sections

Minimal workflow:

```sh
# Start fresh and choose an output file
ripdoc skelebuild reset --output source_map.md

# Add a few key items (prefer paths from `ripdoc list ... --search ... --search-spec path`)
ripdoc skelebuild add ./path/to/crate crate::module::Type
ripdoc skelebuild add ./path/to/crate crate::module::Type::method --implementation

# Add instructions/notes near a specific target
ripdoc skelebuild inject '## Notes\nInvestigate error handling here.' --after-target crate::module::Type
```

The state is stored at `~/.local/state/ripdoc/skelebuild.json` so you can incrementally refine the source map across runs.

## Search Mode

Use the `--search`|`-s` flag with the `print` command to query specific items instead of printing an entire crate. The query returns public API and their ancestors for context.

```sh
# Show methods and fields matching "status" within the reqwest crate
ripdoc print reqwest --search status --search-spec name,signature
```

By default the query matches the name, doc, and signature domains, case-insensitively.

### OR Searches

Use the pipe character `|` to search for multiple terms with OR logic:

```sh
# Find items matching "init" OR "clone" OR "fetch" OR "remote" OR "config"
ripdoc print gix --search "init|clone|fetch|remote|config"

# Search for multiple method names
ripdoc list tokio --search "spawn|block_on|sleep"
```

The OR pattern works across all search domains (names, docs, paths, signatures).

Add `--direct-match-only`|`-d` when you want container matches (modules, structs, traits) to stay collapsed and show only the exact hits.

## Listing Mode

Use the `list` subcommand to print a concise catalog of crate items instead of rendering Rust code. Each line reports the item kind and its fully qualified path, e.g.:

```sh
ripdoc list tokio

crate  tokio         tokio-1.48.0/src/lib.rs:1
module tokio::io     tokio-1.48.0/src/io/mod.rs:1
module tokio::net    tokio-1.48.0/src/net/mod.rs:1
module tokio::task   tokio-1.48.0/src/task/mod.rs:1
module tokio::stream tokio-1.48.0/src/lib.rs:640
macro  tokio::pin    tokio-1.48.0/src/macros/pin.rs:125
```

Filter listing output with `--search`. The listing honours `--private` and feature flags. Each row includes the source file and line.

Below is an example from the `pandoc` crate showing how Ripdoc prints the same snippet in Markdown (default) and in the raw Rust skeleton (`--format rs`):

### Markdown preview (default):

````markdown
```rust
impl Pandoc {
```

Get a new Pandoc object This function returns a builder object to configure the Pandoc execution.

```rust
pub fn new() -> Pandoc {}
```

Add a path hint to search for the LaTeX executable.

The supplied path is searched first for the latex executable, then the environment variable `PATH`, then some hard-coded location hints.

```rust
pub fn add_latex_path_hint<T: AsRef<Path> + ?Sized>(&mut self, path: &T) -> &mut Pandoc {}
```

Add a path hint to search for the Pandoc executable.

The supplied path is searched first for the Pandoc executable, then the environment variable `PATH`, then some hard-coded location hints.

```rust
pub fn add_pandoc_path_hint<T: AsRef<Path> + ?Sized>(&mut self, path: &T) -> &mut Pandoc {}

// Set or overwrite the document-class.
pub fn set_doc_class(&mut self, class: DocumentClass) -> &mut Pandoc {}
```
````

### Rust preview (`--format rs`):

```rust
impl Pandoc {
    /// Get a new Pandoc object
    /// This function returns a builder object to configure the Pandoc
    /// execution.
    pub fn new() -> Pandoc {}

    /// Add a path hint to search for the LaTeX executable.
    ///
    /// The supplied path is searched first for the latex executable, then the environment variable
    /// `PATH`, then some hard-coded location hints.
    pub fn add_latex_path_hint<T: AsRef<Path> + ?Sized>(&mut self, path: &T) -> &mut Pandoc {}

    /// Add a path hint to search for the Pandoc executable.
    ///
    /// The supplied path is searched first for the Pandoc executable, then the environment variable `PATH`, then
    /// some hard-coded location hints.
    pub fn add_pandoc_path_hint<T: AsRef<Path> + ?Sized>(&mut self, path: &T) -> &mut Pandoc {}

    /// Set or overwrite the document-class.
    pub fn set_doc_class(&mut self, class: DocumentClass) -> &mut Pandoc {}
```

Ripdoc prints Markdown by default as it is more token efficient. The output is immediately usable for feeding to LLMs.

## Print READMEs

In addition to printing crate API, Ripdoc can also fetch and print the README file for a crate.

```sh
ripdoc readme tokio
```

## Other Features

- Character highlighting for query hits
- Print raw JSON data for usage with `jq` or similar
- Cache rustdoc JSON on disk automatically (override location via `RIPDOC_CACHE_DIR`)

---

## Requirements

Ripdoc requires the Rust nightly toolchain for its operation:

- **Nightly toolchain**: Required for unstable rustdoc features used to generate JSON documentation

Install the nightly toolchain:

```sh
rustup toolchain install nightly
```

## Usage

Basic usage:

```sh
# Prints the entire public API of the target crate
ripdoc print [TARGET]
```

See the help output for all options:

```sh
ripdoc --help
```

Ripdoc has a flexible target specification that tries to do the right thing in a wide set of circumstances.

```sh
# Current project
ripdoc print

# If we're in a workspace and we have a crate mypackage
ripdoc print mypackage

# A dependency of the current project, else we fetch from crates.io
ripdoc print serde

# A sub-path within a crate
ripdoc print serde::de::Deserialize

# Path to a crate
ripdoc print /my/path

# A module within that crate
ripdoc print /my/path::foo

# A crate from crates.io with a specific version
ripdoc print serde@1.0.0

# Search for "status" across names, signatures and doc comments
ripdoc print reqwest --search status

# Search for "status" in only names and signatures
ripdoc print reqwest --search status --search-spec name,signature

# Search for "status" in docs only
ripdoc print reqwest --search status --search-spec doc

# List public API items
ripdoc list serde

# Print Markdown output with stripped doc comment markers
ripdoc print serde --format markdown
```

---

## Attribution

This crate is a forked and re-worked version of [cortesi's `ruskel`](https://github.com/cortesi/ruskel). Much of its core code is still in use.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
