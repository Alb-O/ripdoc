# ripdoc

![Discord](https://img.shields.io/discord/1381424110831145070?style=flat-square&logo=rust&link=https%3A%2F%2Fdiscord.gg%2FfHmRmuBDxF)
[![Crates.io](https://img.shields.io/crates/v/libripdoc.svg)](https://crates.io/crates/libripdoc)
[![Documentation](https://docs.rs/libripdoc/badge.svg)](https://docs.rs/libripdoc)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Ripdoc produces a syntactically correct, single-page skeleton of a crate's
public API. If the crate is not found in the local workspace, it is fetched
from [crates.io](https://crates.io).

Ripdoc is great for:

- Quick access to Rust documentation from the command line.
- Exporting the full public API of a crate as a single file to pass to LLMs and
  other tools.

For example, here is the skeleton of the very tiny `termsize` crate. Note that
the entire public API is included, but all implementation is omitted.

---

## Search Mode

Use `--search` to focus on specific items instead of rendering an entire crate.
The query runs across multiple domains and returns a skeleton containing only
the matches and their ancestors.

```sh
# Show methods and fields matching "status" within the reqwest crate
ripdoc reqwest --search status --search-spec name,signature
```

By default the query matches the name, doc, and signature domains with case-insensitive
comparisons. Include the optional `path` domain when you need canonical path
matches by passing `--search-spec name,path`, or use `--search-spec doc` to
inspect documentation only. Combine with `--search-case-sensitive` to require
exact letter case.
Add `--direct-match-only` when you want container matches (modules, structs, traits) to stay
collapsed and show only the exact hits.

The search output respects existing flags like `--private`, feature controls, and
syntax highlighting options.

## Listing Mode

Use `--list` to print a concise catalog of crate items instead of rendering
Rust code. Each line reports the item kind and its fully qualified path:

```sh
# Survey the high-level structure of tokio without emitting code
ripdoc tokio --list

crate      crate
module     crate::sync
struct     crate::sync::Mutex
trait      crate::io::AsyncRead
```

Combine `--list` with `--search` to filter the catalog using the same domain
controls as skeleton search. The listing honours `--private` and feature flags,
and it conflicts with `--raw` because the output is tabular text rather than
Rust code.

````rust
pub mod termsize {
    //! Termsize is a tiny crate that provides a simple
    //! interface for retrieving the current
    //! [terminal interface](http://www.manpagez.com/man/4/tty/) size
    //!
    //! ```rust
    //! extern crate termsize;
    //!
    //! termsize::get().map(|size| println!("rows {} cols {}", size.rows, size.cols));
    //! ```

    /// Container for number of rows and columns
    #[derive(Debug)]
    pub struct Size {
        pub rows: u16,
        pub cols: u16,
    }

    /// Gets the current terminal size
    pub fn get() -> Option<self::super::Size> {}
}
````


---

## Features

- Generate a skeletonized view of any Rust crate
- Support for both local crates and remote crates from crates.io
- Filter output to matched items using `--search` with the `--search-spec` domain selector and
  `--direct-match-only` when you want to avoid container expansion
- Generate tabular item listings with `--list`, optionally filtered by `--search`
- Syntax highlighting for terminal output
- Optionally include private items and auto-implemented traits
- Support for custom feature flags and version specification

---

## Requirements

Ripdoc requires the Rust nightly toolchain for its operation:

- **Nightly toolchain**: Required for unstable rustdoc features used to generate JSON documentation

Install the nightly toolchain:
```sh
rustup toolchain install nightly
```

---

## Installation

To install Ripdoc, run:

```sh
cargo install ripdoc
```

Note: While ripdoc requires the nightly toolchain to run, you can install it using any toolchain.


---

## Usage


Basic usage:

```sh
ripdoc [TARGET]
```

See the help output for all options:

```sh
ripdoc --help
```

Ripdoc has a flexible target specification that tries to do the right thing in
a wide set of circumstances.

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
```

---

## libripdoc library

`libripdoc` is a library that can be integrated into other Rust projects to
provide Ripdoc functionality.

Here's a basic example of using `libripdoc` in your Rust code:

```rust
use libripdoc::Ripdoc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rs = Ripdoc::new("/path/to/target")?;
    let rendered = rs.render(false, false)?;
    println!("{}", rendered);
    Ok(())
}
```
