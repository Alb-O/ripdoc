pub use self::config::{create_quiet_cargo_config, is_rustup_available};
pub use self::manifest::{generate_dummy_manifest, to_import_name};
pub use self::path::{CargoPath, create_dummy_crate};
pub use self::resolved_target::{ResolvedTarget, resolve_target};
pub use self::rustdoc_error::map_rustdoc_build_error;

/// Cargo configuration utilities for quiet operation and rustup detection.
pub mod config;
/// Manifest generation for temporary/dummy crates.
pub mod manifest;
/// CargoPath type and cargo crate path resolution.
pub mod path;
/// Target resolution to ResolvedTarget type.
pub mod resolved_target;
/// Rustdoc error handling and diagnostics extraction.
pub mod rustdoc_error;
