use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use semver::Version;
use ureq::http;

use super::path::CargoPath;
use crate::error::{Result, RipdocError};

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";

/// Download (or reuse a cached) crate from crates.io and expose it as a [`CargoPath`].
pub fn fetch_registry_crate(
	name: &str,
	version: Option<&Version>,
	offline: bool,
) -> Result<CargoPath> {
	let resolved_version = if let Some(version) = version {
		version.to_string()
	} else {
		if offline {
			return Err(RipdocError::Generate(format!(
				"crate '{name}' requires an explicit version when running offline"
			)));
		}
		fetch_latest_version(name)?
	};

	// Check if crate exists in cargo's cache
	if let Some(cached_path) = find_in_cargo_cache(name, &resolved_version)? {
		return Ok(CargoPath::Path(cached_path));
	}

	if offline {
		return Err(RipdocError::Generate(format!(
			"crate '{name}'@{resolved_version} is not cached locally for offline use. \
             Run without --offline or use `cargo fetch` first."
		)));
	}

	// Use cargo fetch to download the crate
	fetch_with_cargo(name, &resolved_version)?;

	// Find it in the cache (it should be there now)
	find_in_cargo_cache(name, &resolved_version)?
		.map(CargoPath::Path)
		.ok_or_else(|| {
			RipdocError::Generate(format!(
				"Failed to locate '{name}'@{resolved_version} in cargo cache after download"
			))
		})
}

fn fetch_latest_version(name: &str) -> Result<String> {
	let url = format!("{CRATES_IO_API}/{name}");
	let mut response = request(&url, name)?;

	let mut body = String::new();
	response
		.body_mut()
		.as_reader()
		.read_to_string(&mut body)
		.map_err(|err| {
			RipdocError::Generate(format!(
				"Failed to read crates.io response for '{name}': {err}"
			))
		})?;

	let value: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
		RipdocError::Generate(format!(
			"Failed to parse crates.io metadata for '{name}': {err}"
		))
	})?;

	let crate_info = value
		.get("crate")
		.and_then(|v| v.as_object())
		.ok_or_else(|| {
			RipdocError::Generate(format!("Malformed crates.io response for '{name}'"))
		})?;

	let max_stable = crate_info
		.get("max_stable_version")
		.and_then(|v| v.as_str())
		.filter(|version| !version.is_empty());
	let max_version = crate_info
		.get("max_version")
		.and_then(|v| v.as_str())
		.ok_or_else(|| {
			RipdocError::Generate(format!("Missing max_version for '{name}' on crates.io"))
		})?;

	let chosen = max_stable.unwrap_or(max_version).to_string();

	Ok(chosen)
}

/// Find a crate in cargo's registry cache
fn find_in_cargo_cache(name: &str, version: &str) -> Result<Option<PathBuf>> {
	let cargo_home = get_cargo_home()?;
	let registry_src = cargo_home.join("registry").join("src");

	if !registry_src.exists() {
		return Ok(None);
	}

	// Look for the crate in any of the registry source directories
	// The directory name format is: index.crates.io-<hash>
	for entry in fs::read_dir(&registry_src)? {
		let entry = entry?;
		let index_dir = entry.path();
		if !index_dir.is_dir() {
			continue;
		}

		let crate_dir = index_dir.join(format!("{name}-{version}"));
		if crate_dir.exists() && crate_dir.join("Cargo.toml").exists() {
			return Ok(Some(crate_dir));
		}
	}

	Ok(None)
}

/// Find the latest cached version of a crate in cargo's registry cache.
/// Returns the path to the crate directory and the version string.
pub fn find_latest_cached_version(name: &str) -> Result<Option<(PathBuf, String)>> {
	let cargo_home = get_cargo_home()?;
	let registry_src = cargo_home.join("registry").join("src");

	if !registry_src.exists() {
		return Ok(None);
	}

	let mut found_versions: Vec<(PathBuf, Version)> = Vec::new();

	// Look for all versions of the crate in the registry source directories
	for entry in fs::read_dir(&registry_src)? {
		let entry = entry?;
		let index_dir = entry.path();
		if !index_dir.is_dir() {
			continue;
		}

		// Scan the index directory for crates matching the name pattern
		if let Ok(crates) = fs::read_dir(&index_dir) {
			for crate_entry in crates {
				let crate_entry = crate_entry?;
				let crate_dir = crate_entry.path();
				if !crate_dir.is_dir() {
					continue;
				}

				// Parse the directory name: <crate-name>-<version>
				if let Some(dir_name) = crate_dir.file_name().and_then(|n| n.to_str()) {
					let prefix = format!("{}-", name);
					if let Some(version_str) = dir_name.strip_prefix(&prefix) {
						// Verify it has a Cargo.toml
						if crate_dir.join("Cargo.toml").exists() {
							// Try to parse the version
							if let Ok(version) = Version::parse(version_str) {
								found_versions.push((crate_dir, version));
							}
						}
					}
				}
			}
		}
	}

	if found_versions.is_empty() {
		return Ok(None);
	}

	// Sort by version and take the latest
	found_versions.sort_by(|(_, a), (_, b)| b.cmp(a));
	let (path, version) = found_versions.into_iter().next().unwrap();

	Ok(Some((path, version.to_string())))
}

/// Use `cargo fetch` to download a crate into cargo's cache
fn fetch_with_cargo(name: &str, version: &str) -> Result<()> {
	// Create a temporary directory with a minimal Cargo.toml
	let temp_dir = tempfile::tempdir()
		.map_err(|err| RipdocError::Generate(format!("Failed to create temp directory: {err}")))?;

	let manifest_path = temp_dir.path().join("Cargo.toml");
	let manifest_content = format!(
		r#"[package]
name = "temp-fetch"
version = "0.0.0"
edition = "2021"

[dependencies]
{name} = "={version}"
"#
	);

	fs::write(&manifest_path, manifest_content)
		.map_err(|err| RipdocError::Generate(format!("Failed to write temp Cargo.toml: {err}")))?;

	// Create a minimal src/lib.rs to satisfy cargo's requirement for targets
	let src_dir = temp_dir.path().join("src");
	fs::create_dir(&src_dir)
		.map_err(|err| RipdocError::Generate(format!("Failed to create src directory: {err}")))?;
	let lib_path = src_dir.join("lib.rs");
	fs::write(&lib_path, "")
		.map_err(|err| RipdocError::Generate(format!("Failed to write src/lib.rs: {err}")))?;

	// Run cargo fetch
	let output = Command::new("cargo")
		.arg("fetch")
		.arg("--manifest-path")
		.arg(&manifest_path)
		.output()
		.map_err(|err| RipdocError::Generate(format!("Failed to run cargo fetch: {err}")))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(RipdocError::Generate(format!(
			"cargo fetch failed for '{name}'@{version}: {stderr}"
		)));
	}

	Ok(())
}

fn get_cargo_home() -> Result<PathBuf> {
	if let Some(cargo_home) = env::var_os("CARGO_HOME") {
		return Ok(PathBuf::from(cargo_home));
	}
	if let Some(home) = env::var_os("HOME") {
		return Ok(Path::new(&home).join(".cargo"));
	}

	Err(RipdocError::Generate(
		"Could not determine CARGO_HOME directory".to_string(),
	))
}

/// Fetch the README content for a crate from crates.io.
pub fn fetch_readme(name: &str, version: Option<&Version>) -> Result<String> {
	let resolved_version = if let Some(version) = version {
		version.to_string()
	} else {
		fetch_latest_version(name)?
	};

	let url = format!("{CRATES_IO_API}/{name}/{resolved_version}/readme");
	let mut response = request(&url, name)?;

	let mut body = String::new();
	response
		.body_mut()
		.as_reader()
		.read_to_string(&mut body)
		.map_err(|err| {
			RipdocError::Generate(format!(
				"Failed to read README response for '{name}': {err}"
			))
		})?;

	Ok(body)
}

fn request(url: &str, crate_name: &str) -> Result<http::Response<ureq::Body>> {
	ureq::get(url).call().map_err(|err| match err {
		ureq::Error::StatusCode(404) => RipdocError::ModuleNotFound(crate_name.to_string()),
		err => RipdocError::Generate(format!(
			"Failed to reach crates.io for '{crate_name}': {err}"
		)),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn offline_requires_version() {
		let err = fetch_registry_crate("serde", None, true).unwrap_err();
		assert!(
			err.to_string().contains("requires an explicit version"),
			"unexpected error {err}"
		);
	}

	#[test]
	fn get_cargo_home_respects_env() {
		let original = env::var_os("CARGO_HOME");
		let tmp = tempfile::tempdir().unwrap();

		unsafe {
			env::set_var("CARGO_HOME", tmp.path());
		}

		let cargo_home = get_cargo_home().unwrap();
		assert_eq!(cargo_home, tmp.path());

		unsafe {
			if let Some(original) = original {
				env::set_var("CARGO_HOME", original);
			} else {
				env::remove_var("CARGO_HOME");
			}
		}
	}

	#[test]
	fn find_in_cache_returns_none_when_not_found() {
		let result = find_in_cargo_cache("nonexistent-crate-xyz", "99.99.99").unwrap();
		assert!(result.is_none());
	}
}
