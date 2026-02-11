//! Backend selection for core operations.
//!
//! Default is rustdoc-json. Enable the tree-sitter v2 backend by building with
//! `--features v2-ts` and setting `RIPDOC_BACKEND=ts`.

/// Concrete backend used to satisfy ripdoc API requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
	/// Existing rustdoc-json backend.
	Rustdoc,
	/// v2 tree-sitter backend (feature-gated).
	#[cfg(feature = "v2-ts")]
	TreeSitter,
}

/// Determine the active backend from environment.
/// - `RIPDOC_BACKEND=ts|treesitter|tree-sitter` selects v2 (when compiled).
/// - Anything else defaults to rustdoc.
pub fn active_backend() -> BackendKind {
	let raw = std::env::var("RIPDOC_BACKEND").unwrap_or_default();
	let v = raw.trim().to_ascii_lowercase();
	if matches!(v.as_str(), "ts" | "treesitter" | "tree-sitter") {
		#[cfg(feature = "v2-ts")]
		return BackendKind::TreeSitter;
	}
	BackendKind::Rustdoc
}
