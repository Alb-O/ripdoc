use std::io::Read;
use std::path::{Path, PathBuf};
use std::{env, fs};

use flate2::read::GzDecoder;
use semver::Version;
use tar::Archive;

use super::path::CargoPath;
use crate::error::{Result, RuskelError};

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";

/// Download (or reuse a cached copy of) a crate from crates.io and expose it as a [`CargoPath`].
pub fn fetch_registry_crate(
	name: &str,
	version: Option<&Version>,
	offline: bool,
) -> Result<CargoPath> {
	let resolved_version = if let Some(version) = version {
		version.to_string()
	} else {
		if offline {
			return Err(RuskelError::Generate(format!(
				"crate '{name}' requires an explicit version when running offline"
			)));
		}
		fetch_latest_version(name)?
	};

	let cache_dir = registry_cache_dir()?.join(format!("{name}-{resolved_version}"));
	let manifest_path = cache_dir.join("Cargo.toml");

	if manifest_path.exists() {
		return Ok(CargoPath::Path(cache_dir));
	}

	if offline {
		return Err(RuskelError::Generate(format!(
			"crate '{name}'@{resolved_version} is not cached locally for offline use. \
             Run without --offline or use `cargo fetch {name}` first."
		)));
	}

	download_and_extract(name, &resolved_version, &cache_dir)?;

	Ok(CargoPath::Path(cache_dir))
}

fn fetch_latest_version(name: &str) -> Result<String> {
	let url = format!("{CRATES_IO_API}/{name}");
	let response = request(&url, name)?;

	let mut body = String::new();
	response
		.into_reader()
		.read_to_string(&mut body)
		.map_err(|err| {
			RuskelError::Generate(format!(
				"Failed to read crates.io response for '{name}': {err}"
			))
		})?;

	let value: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
		RuskelError::Generate(format!(
			"Failed to parse crates.io metadata for '{name}': {err}"
		))
	})?;

	let crate_info = value
		.get("crate")
		.and_then(|v| v.as_object())
		.ok_or_else(|| {
			RuskelError::Generate(format!("Malformed crates.io response for '{name}'"))
		})?;

	let max_stable = crate_info
		.get("max_stable_version")
		.and_then(|v| v.as_str())
		.filter(|version| !version.is_empty());
	let max_version = crate_info
		.get("max_version")
		.and_then(|v| v.as_str())
		.ok_or_else(|| {
			RuskelError::Generate(format!("Missing max_version for '{name}' on crates.io"))
		})?;

	let chosen = max_stable.unwrap_or(max_version).to_string();

	Ok(chosen)
}

fn download_and_extract(name: &str, version: &str, destination: &Path) -> Result<()> {
	let download_url = format!("{CRATES_IO_API}/{name}/{version}/download");
	let response = request(&download_url, name)?;

	let mut archive_bytes = Vec::new();
	let mut reader = response.into_reader();
	reader.read_to_end(&mut archive_bytes).map_err(|err| {
		RuskelError::Generate(format!("Failed to download crate '{name}': {err}"))
	})?;

	let parent = destination
		.parent()
		.ok_or_else(|| RuskelError::Generate("Invalid cache directory".to_string()))?;
	fs::create_dir_all(parent)?;

	let staging = tempfile::Builder::new()
		.prefix("download-")
		.tempdir_in(parent)
		.map_err(|err| RuskelError::Generate(format!("Failed to create cache dir: {err}")))?;

	let cursor = std::io::Cursor::new(archive_bytes);
	let gz = GzDecoder::new(cursor);
	let mut archive = Archive::new(gz);
	archive.unpack(staging.path()).map_err(|err| {
		RuskelError::Generate(format!("Failed to unpack crate '{name}'@{version}: {err}"))
	})?;

	let extracted = staging.path().join(format!("{name}-{version}"));
	if !extracted.exists() {
		return Err(RuskelError::Generate(format!(
			"Downloaded archive for '{name}'@{version} did not contain expected directory"
		)));
	}

	if let Err(err) = fs::rename(&extracted, destination) {
		return Err(RuskelError::Generate(format!(
			"Failed to move crate '{name}' into cache: {err}"
		)));
	}

	Ok(())
}

fn registry_cache_dir() -> Result<PathBuf> {
	let mut base = env::var_os("RUSKEL_CACHE_DIR").map(PathBuf::from);
	if base.is_none() {
		if let Some(xdg) = env::var_os("XDG_CACHE_HOME") {
			base = Some(Path::new(&xdg).join("ruskel"));
		} else if let Some(local) = env::var_os("LOCALAPPDATA") {
			base = Some(Path::new(&local).join("ruskel"));
		} else if let Some(home) = env::var_os("HOME") {
			base = Some(Path::new(&home).join(".cache").join("ruskel"));
		}
	}

	let base = base.unwrap_or_else(|| env::temp_dir().join("ruskel-cache"));
	let registry = base.join("registry");
	fs::create_dir_all(&registry).map_err(|err| {
		RuskelError::Generate(format!(
			"Failed to create registry cache directory '{}': {err}",
			registry.display()
		))
	})?;
	Ok(registry)
}

fn request(url: &str, crate_name: &str) -> Result<ureq::Response> {
	match ureq::get(url).call() {
		Ok(resp) => Ok(resp),
		Err(ureq::Error::Status(404, _)) => {
			Err(RuskelError::ModuleNotFound(crate_name.to_string()))
		}
		Err(err) => Err(RuskelError::Generate(format!(
			"Failed to reach crates.io for '{crate_name}': {err}"
		))),
	}
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
	fn cache_dir_uses_env_override() -> Result<()> {
		let tmp = tempfile::tempdir()?;
		unsafe {
			env::set_var("RUSKEL_CACHE_DIR", tmp.path());
		}
		let dir = registry_cache_dir()?;
		assert!(dir.starts_with(tmp.path()));
		unsafe {
			env::remove_var("RUSKEL_CACHE_DIR");
		}
		Ok(())
	}
}
