#![warn(missing_docs)]
//! Ruskel generates skeletonized versions of Rust crates.
//!
//! It produces a single-page, syntactically valid Rust code representation of a crate,
//! with all implementations omitted. This provides a clear overview of the crate's structure
//! and public API.
//!
//! Ruskel works by first fetching all dependencies, then using the nightly Rust toolchain
//! to generate JSON documentation data. This data is then parsed and rendered into
//! the skeletonized format. The skeletonized code is then formatted with rustfmt.
//!
//!
//! You must have the nightly Rust toolchain installed to use (but not to install) Ruskel.

/// Helper utilities for querying Cargo metadata and managing crate sources.
pub mod cargo;
/// Utilities for normalising rustdoc structures before rendering.
mod crateutils;
/// Error types exposed by the libruskel crate.
mod error;
/// Identifier helpers shared across rendering code.
mod keywords;
/// Rendering logic that turns rustdoc data into skeleton code.
mod render;
/// Public API surface for driving the renderer.
mod ruskel;
/// Search and indexing utilities used by the CLI.
pub mod search;
/// Signature rendering utilities for compact item declarations.
mod signature;
/// Target parsing helpers for user-provided specifications.
mod target;
/// Test utilities shared across test modules.
#[cfg(test)]
mod testutils;

pub use ruskel::Ruskel;

pub use crate::error::{Result, RuskelError};
pub use crate::render::Renderer;
pub use crate::search::{
	ListItem, SearchDomain, SearchIndex, SearchItemKind, SearchOptions, SearchPathSegment,
	SearchResponse, SearchResult, describe_domains,
};
