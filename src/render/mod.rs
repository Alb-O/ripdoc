//! Rendering logic that converts rustdoc data into skeleton Rust code.
//!
//! This module handles the transformation of rustdoc JSON output into skeleton code representation.

/// Convenience macro to destructure `rustdoc_types::Item` variants during rendering.
#[macro_export]
macro_rules! extract_item {
    ($item:expr, $variant:path) => {
        match &$item.inner {
            $variant(inner) => inner,
            _ => panic!("Expected {}, found {:?}", stringify!($variant), $item.inner),
        }
    };
    ($item:expr, $variant:path { $($field:ident),+ }) => {
        match &$item.inner {
            $variant { $($field,)+ .. } => ($($field,)+),
            _ => panic!("Expected {}, found {:?}", stringify!($variant), $item.inner),
        }
    };
}

/// Syntax utilities for rendering items, types, and paths.
pub mod syntax;

/// Main renderer configuration and public API.
pub mod core;
/// Domain-specific errors for the renderer.
pub mod error;
/// Trait and impl rendering logic.
pub mod impls;
/// Item-specific rendering functions.
pub mod items;
/// Procedural and declarative macro rendering.
pub mod macros;
/// Markdown conversion helpers.
pub mod markdown;
/// Signature rendering utilities for Rust items.
pub mod signatures;
/// Mutable rendering state and filtering.
pub mod state;
/// Utility functions for rendering.
pub mod utils;

// Re-export public API
pub use core::{RenderFormat, RenderSelection, Renderer};

pub use syntax::{
	is_reserved_word, render_function_args, render_generic_bounds, render_generics, render_name,
	render_path, render_return_type, render_type, render_type_inner, render_vis,
	render_where_clause,
};
