//! Rendering logic that converts rustdoc data into skeleton Rust code.
//!
//! This module handles the transformation of rustdoc JSON output into skeleton code representation.

/// Main renderer configuration and public API.
pub mod core;
/// Trait and impl rendering logic.
pub mod impls;
/// Item-specific rendering functions.
pub mod items;
/// Procedural and declarative macro rendering.
pub mod macros;
/// Mutable rendering state and filtering.
pub mod state;
/// Utility functions for rendering.
pub mod utils;

// Re-export public API
pub use core::{RenderSelection, Renderer};
