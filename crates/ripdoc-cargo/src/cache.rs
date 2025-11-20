//! Caching layer for rendered crate documentation.
//!
//! Provides a disk-based cache for rustdoc JSON output to avoid
//! expensive re-generation of documentation for the same crate.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::{env, fs};

use rustdoc_types::Crate;

use crate::error::{Result, RipdocError};

/// Configuration for the documentation cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
	/// Whether caching is enabled.
	pub enabled: bool,
	/// Directory where cached documentation is stored.
	/// If None, uses the default cache directory.
	pub cache_dir: Option<PathBuf>,
}

impl Default for CacheConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			cache_dir: None,
		}
	}
}

impl CacheConfig {
	/// Create a new cache configuration with caching enabled.
	pub fn new() -> Self {
		Self::default()
	}

	/// Disable caching.
	pub fn disabled() -> Self {
		Self {
			enabled: false,
			cache_dir: None,
		}
	}

	/// Set a custom cache directory.
	pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
		self.cache_dir = Some(dir);
		self
	}

	/// Get the cache directory, using the default if not specified.
	fn get_cache_dir(&self) -> Result<PathBuf> {
		if let Some(ref dir) = self.cache_dir {
			return Ok(dir.clone());
		}

		if let Ok(dir) = env::var("RIPDOC_CACHE_DIR") {
			return Ok(PathBuf::from(dir));
		}

		// Use platform-specific cache directory via the dirs crate
		let cache_base = dirs::cache_dir().ok_or_else(|| {
			RipdocError::Generate("Could not determine cache directory".to_string())
		})?;

		Ok(cache_base.join("ripdoc"))
	}
}

/// Parameters that affect the cache key for a crate build.
#[derive(Debug)]
pub struct CacheKey {
	/// Package name and version from Cargo.toml.
	pub package_info: String,
	/// Absolute path to the manifest (for local crates).
	pub manifest_path: PathBuf,
	/// Whether default features are disabled.
	pub no_default_features: bool,
	/// Whether all features are enabled.
	pub all_features: bool,
	/// List of specific features to enable.
	pub features: Vec<String>,
	/// Whether private items are included.
	pub private_items: bool,
	/// Rust toolchain version (to handle rustdoc JSON format changes).
	pub toolchain_version: Option<String>,
}

impl CacheKey {
	/// Generate a cache key from build parameters.
	pub fn new(
		manifest_path: PathBuf,
		package_info: String,
		no_default_features: bool,
		all_features: bool,
		mut features: Vec<String>,
		private_items: bool,
		toolchain_version: Option<String>,
	) -> Self {
		// Sort features for consistent cache keys
		features.sort();

		Self {
			package_info,
			manifest_path,
			no_default_features,
			all_features,
			features,
			private_items,
			toolchain_version,
		}
	}

	/// Compute a stable hash for this cache key.
	fn hash(&self) -> String {
		let mut hasher = DefaultHasher::new();

		// Hash the manifest path
		self.manifest_path.hash(&mut hasher);

		// Hash package info
		self.package_info.hash(&mut hasher);

		// Hash build flags
		self.no_default_features.hash(&mut hasher);
		self.all_features.hash(&mut hasher);
		self.private_items.hash(&mut hasher);

		// Hash features
		self.features.hash(&mut hasher);

		// Hash toolchain version
		self.toolchain_version.hash(&mut hasher);

		format!("{:x}", hasher.finish())
	}

	/// Get the cache file path for this key.
	fn cache_path(&self, cache_dir: &Path) -> PathBuf {
		let hash = self.hash();
		cache_dir.join(format!("{}.bin", hash))
	}
}

/// Try to load cached documentation for the given parameters.
pub fn load_cached(config: &CacheConfig, key: &CacheKey) -> Result<Option<Crate>> {
	if !config.enabled {
		return Ok(None);
	}

	let cache_dir = config.get_cache_dir()?;
	let cache_path = key.cache_path(&cache_dir);

	if !cache_path.exists() {
		return Ok(None);
	}

	// Try to load and deserialize the cached data
	let data = fs::read(&cache_path).map_err(|e| {
		RipdocError::Generate(format!(
			"Failed to read cache file {}: {}",
			cache_path.display(),
			e
		))
	})?;

	let config = bincode::config::standard();
	let (crate_data, _len): (Crate, usize) = bincode::serde::decode_from_slice(&data, config)
		.map_err(|e| {
			// If deserialization fails, the cache is likely stale or corrupted
			// Delete it and return None
			let _ = fs::remove_file(&cache_path);
			RipdocError::Generate(format!(
				"Cache deserialization failed (removing stale cache): {}",
				e
			))
		})?;

	Ok(Some(crate_data))
}

/// Save documentation to the cache.
pub fn save_cached(config: &CacheConfig, key: &CacheKey, crate_data: &Crate) -> Result<()> {
	if !config.enabled {
		return Ok(());
	}

	let cache_dir = config.get_cache_dir()?;

	// Create cache directory if it doesn't exist
	fs::create_dir_all(&cache_dir).map_err(|e| {
		RipdocError::Generate(format!(
			"Failed to create cache directory {}: {}",
			cache_dir.display(),
			e
		))
	})?;

	let cache_path = key.cache_path(&cache_dir);

	// Serialize the crate data
	let config = bincode::config::standard();
	let data = bincode::serde::encode_to_vec(crate_data, config)
		.map_err(|e| RipdocError::Generate(format!("Failed to serialize cache data: {}", e)))?;

	// Write to a temporary file first, then rename atomically
	let temp_path = cache_path.with_extension("tmp");
	fs::write(&temp_path, &data).map_err(|e| {
		RipdocError::Generate(format!(
			"Failed to write cache file {}: {}",
			temp_path.display(),
			e
		))
	})?;

	fs::rename(&temp_path, &cache_path).map_err(|e| {
		RipdocError::Generate(format!(
			"Failed to finalize cache file {}: {}",
			cache_path.display(),
			e
		))
	})?;

	Ok(())
}

/// Get the current Rust toolchain version for cache invalidation.
pub fn get_toolchain_version() -> Option<String> {
	use std::process::Command;

	let output = if crate::is_rustup_available() {
		Command::new("rustup")
			.args(["run", "nightly", "rustc", "--version"])
			.output()
			.ok()?
	} else {
		Command::new("rustc").arg("--version").output().ok()?
	};

	if output.status.success() {
		Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_cache_key_hash_consistency() {
		let manifest = PathBuf::from("/path/to/Cargo.toml");
		let key1 = CacheKey::new(
			manifest.clone(),
			"test-crate-0.1.0".to_string(),
			false,
			false,
			vec!["feature1".to_string(), "feature2".to_string()],
			false,
			Some("rustc 1.70.0".to_string()),
		);

		let key2 = CacheKey::new(
			manifest,
			"test-crate-0.1.0".to_string(),
			false,
			false,
			vec!["feature2".to_string(), "feature1".to_string()], // Different order
			false,
			Some("rustc 1.70.0".to_string()),
		);

		// Features should be sorted, so hashes should match
		assert_eq!(key1.hash(), key2.hash());
	}

	#[test]
	fn test_cache_key_hash_different() {
		let manifest = PathBuf::from("/path/to/Cargo.toml");
		let key1 = CacheKey::new(
			manifest.clone(),
			"test-crate-0.1.0".to_string(),
			false,
			false,
			vec![],
			false,
			Some("rustc 1.70.0".to_string()),
		);

		let key2 = CacheKey::new(
			manifest,
			"test-crate-0.1.0".to_string(),
			true, // Different flag
			false,
			vec![],
			false,
			Some("rustc 1.70.0".to_string()),
		);

		assert_ne!(key1.hash(), key2.hash());
	}
}
