# Ripdoc

Ripdoc produces a syntactical outline of a crate's public API and documentation. The CLI provides on-demand access to these resources from any source (local filesystem or through [crates.io](https://crates.io)), perfect for AI agent usage.

## Search Mode

Use `--search`|`-s` to focus on specific items instead of rendering an entire crate. The query runs across multiple domains and returns the public API containing the matches and their ancestors for context.

```sh
# Show methods and fields matching "status" within the reqwest crate
ripdoc reqwest --search status --search-spec name,signature
```

By default the query matches the name, doc, and signature domains with case-insensitive comparisons. Include the optional `path` domain when you need canonical path matches by passing `--search-spec name,path`, or use `--search-spec doc` to inspect documentation only. Combine with `--search-case-sensitive` to require exact letter case.

Add `--direct-match-only`|`-d` when you want container matches (modules, structs, traits) to stay collapsed and show only the exact hits.

The search output respects existing flags like `--private`, feature controls, and syntax highlighting options.

## Listing Mode

Use `--list`|`-l` to print a concise catalog of crate items instead of rendering Rust code. Each line reports the item kind and its fully qualified path:

```sh
# Survey the high-level structure of tokio without emitting code
ripdoc tokio --list

crate      crate
module     crate::sync
struct     crate::sync::Mutex
trait      crate::io::AsyncRead
```

Combine `--list` with `--search` to filter the catalog using the same domain controls as skeleton search. The listing honours `--private` and feature flags, and it conflicts with `--raw` because the output is tabular text rather than Rust code.

Below is a small excerpt from the `pandoc` crate showing how Ripdoc renders the same snippet in Markdown (default) and in the raw Rust skeleton (`--format rs`):

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

Ripdoc renders Markdown by default as it is more token efficient. The output is immediately usable for feeding to LLMs.

## Features

- Support for both local crates and remote crates from crates.io
- Filter output to matched items using `--search` with the `--search-spec` domain selector and `--direct-match-only` when you want to avoid container expansion
- Generate tabular item listings with `--list`, optionally filtered by `--search`
- Search match highlighting for terminal output
- Markdown-friendly output, which strips doc markers and wraps code in fenced `rust` blocks (use `--format rs` for raw Rust output)
- Optionally include private items and auto-implemented traits
- Support for querying against feature flags and version specification

---

## Requirements

Ripdoc requires the Rust nightly toolchain for its operation:

- **Nightly toolchain**: Required for unstable rustdoc features used to generate JSON documentation

Install the nightly toolchain:

```sh
rustup toolchain install nightly
```

## Installation

To install Ripdoc, run:

```sh
cargo install ripdoc
```

Note: While ripdoc requires the nightly toolchain to run, you can install it using any toolchain.

## Usage

Basic usage:

```sh
ripdoc [TARGET]
```

See the help output for all options:

```sh
ripdoc --help
```

Ripdoc has a flexible target specification that tries to do the right thing in a wide set of circumstances.

```sh
# Current project
ripdoc

# If we're in a workspace and we have a crate mypacakage
ripdoc mypackage

# A dependency of the current project, else we fetch from crates.io
ripdoc serde

# A sub-path within a crate
ripdoc serde::de::Deserialize

# Path to a crate
ripdoc /my/path

# A module within that crate
ripdoc /my/path::foo

# A crate from crates.io with a specific version
ripdoc serde@1.0.0

# Search for "status" across names, signatures and doc comments
ripdoc reqwest --search status

# Search for "status" in only names and signatures
ripdoc reqwest --search status --search-spec name,signature

# Search for "status" in docs only
ripdoc reqwest --search status --search-spec doc

# Render Markdown output with stripped doc comment markers
ripdoc serde --format markdown
```

---

## ripdoc-core library

`ripdoc-core` is a library that can be integrated into other Rust projects to provide Ripdoc functionality.

An example of using `ripdoc-core` in your Rust code:

```rust
use ripdoc_core::Ripdoc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ripdoc = Ripdoc::new().with_silent(true);
    let rendered = ripdoc.render(
        "serde",           // target
        false,             // no_default_features
        false,             // all_features
        Vec::new(),        // features
        false              // private_items
    )?;
    println!("{}", rendered);
    Ok(())
}
```

## Attribution

This crate is a forked and re-worked version of [cortesi's `ruskel`](https://github.com/cortesi/ruskel). Much of its core code is still in use.
