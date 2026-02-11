//! Core library for ripdoc, providing the main API for rendering Rust documentation.
//!
//! This crate provides the high-level `Ripdoc` API which orchestrates target resolution,
//! crate documentation generation, and rendering. It is designed to be UI-agnostic and
//! can be used by any frontend (CLI, GUI, language server, etc.).

/// Utilities for querying Cargo metadata and managing crate sources.
pub mod cargo_utils;

/// Rendering logic that converts rustdoc data into skeleton Rust code.
pub mod render;

/// Pseudo-interactive skeleton builder.
pub mod skelebuild;

/// Core API for ripdoc operations.
pub mod core_api;

#[cfg(feature = "v2-ts")]
pub(crate) mod v2;

// Re-export main public API from core_api
// Re-export target parsing from cargo_utils
pub use crate::cargo_utils::target;
pub use crate::core_api::{
	ListTreeNode, RenderFormat, Result, Ripdoc, SearchDomain, SearchItemKind, SearchOptions, SearchResponse, SourceLocation, build_list_tree,
};
