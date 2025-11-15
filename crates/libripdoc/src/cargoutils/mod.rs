pub use self::config::is_rustup_available;
pub use self::manifest::to_import_name;
pub use self::path::CargoPath;
pub use self::registry::fetch_registry_crate;
pub use self::resolved_target::{ResolvedTarget, resolve_target};
pub use self::rustdoc_error::map_rustdoc_build_error;

/// Cargo configuration utilities for quiet operation and rustup detection.
pub mod config;
/// Manifest generation for temporary/dummy crates.
pub mod manifest;
/// CargoPath type and cargo crate path resolution.
pub mod path;
/// Downloading crates from crates.io into a local cache.
pub mod registry;
/// Target resolution to ResolvedTarget type.
pub mod resolved_target;
/// Rustdoc error handling and diagnostics extraction.
pub mod rustdoc_error;
