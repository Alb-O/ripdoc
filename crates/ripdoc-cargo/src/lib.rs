//! Utilities for querying Cargo metadata and managing crate sources.

pub use self::cache::{CacheConfig, CacheKey, get_toolchain_version, load_cached, save_cached};
pub use self::error::{Result, RipdocError};
pub use self::path::CargoPath;
pub use self::registry::fetch_registry_crate;
pub use self::resolved_target::{ResolvedTarget, resolve_target};
pub use self::rustdoc_error::map_rustdoc_build_error;
/// Caching layer for rustdoc JSON output.
pub mod cache;
/// Error helpers for interacting with Cargo and rustdoc.
pub mod error;
/// CargoPath type and cargo crate path resolution.
pub mod path;
/// Downloading crates from crates.io into a local cache.
pub mod registry;
/// Target resolution to ResolvedTarget type.
pub mod resolved_target;
/// Rustdoc error handling and diagnostics extraction.
pub mod rustdoc_error;
/// Target parsing utilities.
pub mod target;

/// Check if rustup is available on the system
pub fn is_rustup_available() -> bool {
	use std::process::{Command, Stdio};
	Command::new("rustup")
		.arg("--version")
		.stderr(Stdio::null())
		.stdout(Stdio::null())
		.status()
		.map(|status| status.success())
		.unwrap_or(false)
}

/// Convert a package name into its canonical import form by replacing hyphens.
pub fn to_import_name(package_name: &str) -> String {
	package_name.replace('-', "_")
}
