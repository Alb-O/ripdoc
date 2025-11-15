#![warn(missing_docs)]
//! Ripdoc generates skeletonized versions of Rust crates.
//!
//! It produces a single-page, syntactically valid Rust code representation of a crate,
//! with all implementations omitted. This provides a clear overview of the crate's structure
//! and public API.
//!
//! Ripdoc works by first fetching all dependencies, then using the nightly Rust toolchain
//! to generate JSON documentation data. This data is then parsed and rendered into
//! the skeletonized format. The skeletonized code is then formatted with rustfmt.
//!
//!
//! You must have the nightly Rust toolchain installed to use (but not to install) Ripdoc.

/// Helper utilities for querying Cargo metadata and managing crate sources.
pub mod cargoutils;
/// Utilities for rendering items and types in skeleton code.
pub mod crateutils;
/// Error types exposed by the libripdoc crate.
mod error;
/// Rendering logic that turns rustdoc data into skeleton code.
pub mod render;
/// Public API surface for driving the renderer.
mod ripdoc;
/// Search and indexing utilities used by the CLI.
pub mod search;
/// Signature rendering utilities for compact item declarations.
mod signature;
/// Target parsing helpers for user-provided specifications.
mod target;
/// Test utilities shared across test modules.
#[cfg(test)]
mod testutils;

pub use ripdoc::Ripdoc;

pub use crate::error::{Result, RipdocError};
pub use crate::render::{RenderSelection, Renderer};
pub use crate::search::{
	ListItem, SearchDomain, SearchIndex, SearchItemKind, SearchOptions, SearchPathSegment,
	SearchResponse, SearchResult, describe_domains,
};

/// Rust reserved words that require raw identifier handling.
pub const RESERVED_WORDS: &[&str] = &[
	"abstract", "as", "become", "box", "break", "const", "continue", "crate", "do", "else", "enum",
	"extern", "false", "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match",
	"mod", "move", "mut", "override", "priv", "pub", "ref", "return", "self", "Self", "static",
	"struct", "super", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
	"virtual", "where", "while", "yield",
];

/// Determine whether `ident` is a Rust keyword that needs escaping.
pub fn is_reserved_word(ident: &str) -> bool {
	RESERVED_WORDS.contains(&ident)
}