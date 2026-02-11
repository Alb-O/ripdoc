#![cfg(feature = "v2-ts")]

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use semver::Version;
use tar::Archive;

use crate::cargo_utils::target::{Entrypoint, Target};
use crate::cargo_utils::to_import_name;
use crate::core_api::Result;
use crate::core_api::error::RipdocError;

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";
const STATIC_CRATE_DL: &str = "https://static.crates.io/crates";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct V2Source {
	pub(crate) root_dir: PathBuf,
	pub(crate) package_name: String,
	pub(crate) crate_name: String,
	pub(crate) entry_file: PathBuf,
	pub(crate) entry_rel_path: PathBuf,
	pub(crate) source_prefix: String,
}

/// Resolve v2 sources from local paths or named crates.
pub(crate) fn resolve_source(target: &str, offline: bool) -> Result<V2Source> {
	let parsed = Target::parse(target)?;
	match parsed.entrypoint {
		Entrypoint::Path(_) => resolve_local_source(target),
		Entrypoint::Name { name, version } => resolve_named_source(&name, version.as_ref(), offline),
	}
}

/// Resolve local path targets.
pub(crate) fn resolve_local_source(target: &str) -> Result<V2Source> {
	let parsed = Target::parse(target)?;
	let Entrypoint::Path(path) = parsed.entrypoint else {
		return Err(RipdocError::InvalidTarget("Expected a local path target".to_string()));
	};

	let abs = if path.is_relative() {
		std::path::absolute(&path).map_err(|e| RipdocError::InvalidTarget(format!("Failed to resolve target path '{}': {e}", path.display())))?
	} else {
		path
	};

	let root_dir = if abs.is_file() {
		let start = abs.parent().unwrap_or(abs.as_path());
		find_manifest_root(start)?
	} else {
		find_manifest_root(&abs)?
	};

	build_source_from_root(root_dir, None)
}

fn resolve_named_source(name: &str, version: Option<&Version>, offline: bool) -> Result<V2Source> {
	let version = match version {
		Some(v) => v.clone(),
		None => {
			if let Some((_, v)) = find_latest_cached_version_anywhere(name)? {
				v
			} else if offline {
				return Err(RipdocError::InvalidTarget(format!(
					"crate '{name}' requires an explicit version when running offline"
				)));
			} else {
				fetch_latest_version(name)?
			}
		}
	};

	if let Some(path) = find_in_cargo_registry_cache(name, &version)? {
		return build_source_from_root(path, None);
	}

	if let Some(path) = find_in_v2_registry_cache(name, &version)? {
		let prefix = format!("{name}-{version}");
		return build_source_from_root(path, Some(prefix));
	}

	if offline {
		return Err(RipdocError::InvalidTarget(format!(
			"crate '{name}'@{version} is not cached locally for offline use.\nTry:\n- specify a version that exists in your cargo cache, or\n- run without --offline to download from crates.io"
		)));
	}

	let extracted = download_and_extract_from_crates_io(name, &version)?;
	let prefix = format!("{name}-{version}");
	build_source_from_root(extracted, Some(prefix))
}

fn build_source_from_root(root_dir: PathBuf, source_prefix_override: Option<String>) -> Result<V2Source> {
	let root_dir = root_dir.canonicalize().unwrap_or(root_dir);
	let manifest_path = root_dir.join("Cargo.toml");
	if !manifest_path.exists() {
		return Err(RipdocError::InvalidTarget(format!("No Cargo.toml found at '{}'", manifest_path.display())));
	}

	let manifest_str =
		fs::read_to_string(&manifest_path).map_err(|e| RipdocError::InvalidTarget(format!("Failed to read manifest '{}': {e}", manifest_path.display())))?;
	let manifest: cargo_toml::Manifest = cargo_toml::Manifest::from_str(&manifest_str)
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to parse manifest '{}': {e}", manifest_path.display())))?;

	let Some(pkg) = manifest.package.as_ref() else {
		return Err(RipdocError::InvalidTarget(format!(
			"Workspace/virtual manifest not supported yet by v2 list: {}",
			manifest_path.display()
		)));
	};

	let package_name = pkg.name.clone();
	let crate_name = to_import_name(&package_name);

	let entry_file = select_entry_file(&root_dir, &manifest)
		.ok_or_else(|| RipdocError::InvalidTarget(format!("Could not determine crate entry file (no lib/bin found) in '{}'", root_dir.display())))?;
	let entry_file = entry_file.canonicalize().unwrap_or(entry_file);
	let entry_rel_path = entry_file.strip_prefix(&root_dir).unwrap_or(entry_file.as_path()).to_path_buf();

	let mut source_prefix = source_prefix_override
		.or_else(|| root_dir.file_name().and_then(|os| os.to_str()).map(|s| s.to_string()))
		.unwrap_or_else(|| crate_name.clone());

	if let Some(version) = pkg.version.get().ok().cloned() {
		let wanted = format!("{}-{}", package_name, version);
		if !source_prefix.contains(version.as_str()) {
			source_prefix = wanted;
		}
	}

	Ok(V2Source {
		root_dir,
		package_name,
		crate_name,
		entry_file,
		entry_rel_path,
		source_prefix,
	})
}

fn select_entry_file(root: &Path, manifest: &cargo_toml::Manifest) -> Option<PathBuf> {
	if let Some(lib) = manifest.lib.as_ref()
		&& let Some(path) = lib.path.as_ref()
	{
		let p = root.join(path);
		if p.exists() {
			return Some(p);
		}
	}

	let lib_default = root.join("src/lib.rs");
	if lib_default.exists() {
		return Some(lib_default);
	}

	if !manifest.bin.is_empty()
		&& let Some(path) = manifest.bin[0].path.as_ref()
	{
		let p = root.join(path);
		if p.exists() {
			return Some(p);
		}
	}

	let main_default = root.join("src/main.rs");
	if main_default.exists() {
		return Some(main_default);
	}

	None
}

fn find_manifest_root(start: &Path) -> Result<PathBuf> {
	let mut cur = start.to_path_buf();
	loop {
		if cur.join("Cargo.toml").exists() {
			return Ok(cur);
		}
		if !cur.pop() {
			break;
		}
	}

	Err(RipdocError::InvalidTarget(
		"Failed to locate Cargo.toml by walking up parent directories".to_string(),
	))
}

fn cargo_home() -> Result<PathBuf> {
	if let Some(v) = std::env::var_os("CARGO_HOME") {
		return Ok(PathBuf::from(v));
	}
	if let Some(home) = std::env::var_os("HOME") {
		return Ok(Path::new(&home).join(".cargo"));
	}

	Err(RipdocError::InvalidTarget("Could not determine CARGO_HOME directory".to_string()))
}

fn find_in_cargo_registry_cache(name: &str, version: &Version) -> Result<Option<PathBuf>> {
	let cargo_home = cargo_home()?;
	let registry_src = cargo_home.join("registry").join("src");
	if !registry_src.exists() {
		return Ok(None);
	}

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

fn find_latest_cached_version_anywhere(name: &str) -> Result<Option<(PathBuf, Version)>> {
	if let Some(found) = find_latest_cached_version_in_cargo_cache(name)? {
		return Ok(Some(found));
	}
	if let Some(found) = find_latest_cached_version_in_v2_cache(name)? {
		return Ok(Some(found));
	}
	Ok(None)
}

fn find_latest_cached_version_in_cargo_cache(name: &str) -> Result<Option<(PathBuf, Version)>> {
	let cargo_home = cargo_home()?;
	let registry_src = cargo_home.join("registry").join("src");
	if !registry_src.exists() {
		return Ok(None);
	}

	let mut found: Vec<(PathBuf, Version)> = Vec::new();

	for entry in fs::read_dir(&registry_src)? {
		let entry = entry?;
		let index_dir = entry.path();
		if !index_dir.is_dir() {
			continue;
		}

		if let Ok(crates) = fs::read_dir(&index_dir) {
			for crate_entry in crates {
				let crate_entry = crate_entry?;
				let crate_dir = crate_entry.path();
				if !crate_dir.is_dir() || !crate_dir.join("Cargo.toml").exists() {
					continue;
				}

				let Some(dir_name) = crate_dir.file_name().and_then(|n| n.to_str()) else {
					continue;
				};
				let prefix = format!("{name}-");
				let Some(version_str) = dir_name.strip_prefix(&prefix) else {
					continue;
				};

				if let Ok(version) = Version::parse(version_str) {
					found.push((crate_dir, version));
				}
			}
		}
	}

	if found.is_empty() {
		return Ok(None);
	}

	found.sort_by(|a, b| b.1.cmp(&a.1));
	Ok(found.into_iter().next())
}

fn v2_registry_cache_root() -> Result<PathBuf> {
	if let Ok(dir) = std::env::var("RIPDOC_CACHE_DIR") {
		return Ok(PathBuf::from(dir).join("v2").join("registry"));
	}

	let base = dirs::cache_dir().ok_or_else(|| RipdocError::InvalidTarget("Could not determine cache directory".to_string()))?;
	Ok(base.join("ripdoc").join("v2").join("registry"))
}

fn v2_registry_cache_dir(name: &str, version: &Version) -> Result<PathBuf> {
	Ok(v2_registry_cache_root()?.join(name).join(version.to_string()))
}

fn find_in_v2_registry_cache(name: &str, version: &Version) -> Result<Option<PathBuf>> {
	let dir = v2_registry_cache_dir(name, version)?;
	if dir.exists() && dir.join("Cargo.toml").exists() {
		return Ok(Some(dir));
	}
	Ok(None)
}

fn find_latest_cached_version_in_v2_cache(name: &str) -> Result<Option<(PathBuf, Version)>> {
	let root = v2_registry_cache_root()?.join(name);
	if !root.exists() {
		return Ok(None);
	}

	let mut found: Vec<(PathBuf, Version)> = Vec::new();
	for entry in fs::read_dir(&root)? {
		let entry = entry?;
		let path = entry.path();
		if !path.is_dir() || !path.join("Cargo.toml").exists() {
			continue;
		}

		let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
			continue;
		};
		if let Ok(version) = Version::parse(dir_name) {
			found.push((path, version));
		}
	}

	if found.is_empty() {
		return Ok(None);
	}

	found.sort_by(|a, b| b.1.cmp(&a.1));
	Ok(found.into_iter().next())
}

fn fetch_latest_version(name: &str) -> Result<Version> {
	let url = format!("{CRATES_IO_API}/{name}");
	let mut response = ureq::get(&url).call().map_err(|err| match err {
		ureq::Error::StatusCode(404) => RipdocError::InvalidTarget(format!("crate '{name}' was not found on crates.io")),
		err => RipdocError::InvalidTarget(format!("Failed to reach crates.io for '{name}': {err}")),
	})?;

	let mut body = String::new();
	response
		.body_mut()
		.as_reader()
		.read_to_string(&mut body)
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to read crates.io response for '{name}': {e}")))?;

	let value: serde_json::Value =
		serde_json::from_str(&body).map_err(|e| RipdocError::InvalidTarget(format!("Failed to parse crates.io metadata for '{name}': {e}")))?;

	let crate_info = value
		.get("crate")
		.and_then(|v| v.as_object())
		.ok_or_else(|| RipdocError::InvalidTarget(format!("Malformed crates.io response for '{name}'")))?;

	let max_stable = crate_info.get("max_stable_version").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
	let max_version = crate_info
		.get("max_version")
		.and_then(|v| v.as_str())
		.ok_or_else(|| RipdocError::InvalidTarget(format!("Missing max_version for '{name}' on crates.io")))?;

	let chosen = max_stable.unwrap_or(max_version);
	Version::parse(chosen).map_err(|e| RipdocError::InvalidTarget(format!("crates.io returned non-semver version '{chosen}' for '{name}': {e}")))
}

fn download_and_extract_from_crates_io(name: &str, version: &Version) -> Result<PathBuf> {
	let dest = v2_registry_cache_dir(name, version)?;
	if dest.exists() && dest.join("Cargo.toml").exists() {
		return Ok(dest);
	}

	if let Some(parent) = dest.parent() {
		fs::create_dir_all(parent)?;
	}

	let cache_root = v2_registry_cache_root()?;
	fs::create_dir_all(&cache_root)?;
	let temp = tempfile::Builder::new()
		.prefix(".tmp-extract-")
		.tempdir_in(&cache_root)
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to create temp dir: {e}")))?;

	download_and_extract_into(name, version, temp.path())?;

	if let Some(parent) = dest.parent() {
		fs::create_dir_all(parent)?;
	}
	if dest.exists() {
		let _ = fs::remove_dir_all(&dest);
	}

	fs::rename(temp.path(), &dest).map_err(|e| RipdocError::InvalidTarget(format!("Failed to finalize extracted crate cache '{}': {e}", dest.display())))?;

	Ok(dest)
}

fn download_and_extract_into(name: &str, version: &Version, dest_dir: &Path) -> Result<()> {
	let url = format!("{STATIC_CRATE_DL}/{name}/{name}-{version}.crate");
	let mut response = ureq::get(&url).call().map_err(|err| match err {
		ureq::Error::StatusCode(404) => RipdocError::InvalidTarget(format!("crate '{name}@{version}' was not found")),
		err => RipdocError::InvalidTarget(format!("Failed to download '{name}@{version}' from crates.io: {err}")),
	})?;

	let reader = response.body_mut().as_reader();
	extract_targz_stripping_prefix(reader, dest_dir)
}

/// Extract a `.crate` stream into `dest_dir`, stripping the archive root directory.
fn extract_targz_stripping_prefix<R: Read>(reader: R, dest_dir: &Path) -> Result<()> {
	fs::create_dir_all(dest_dir)?;
	let gz = GzDecoder::new(reader);
	let mut archive = Archive::new(gz);

	for entry in archive
		.entries()
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to read tar entries: {e}")))?
	{
		let mut entry = entry.map_err(|e| RipdocError::InvalidTarget(format!("Invalid tar entry: {e}")))?;
		let path = entry
			.path()
			.map_err(|e| RipdocError::InvalidTarget(format!("Failed to read tar entry path: {e}")))?;

		let mut components = path.components();
		let _ = components.next();

		let mut rel = PathBuf::new();
		let mut bad = false;
		for component in components {
			match component {
				Component::Normal(part) => rel.push(part),
				_ => {
					bad = true;
					break;
				}
			}
		}

		if bad || rel.as_os_str().is_empty() {
			continue;
		}

		let out = dest_dir.join(&rel);
		if let Some(parent) = out.parent() {
			fs::create_dir_all(parent)?;
		}
		entry
			.unpack(&out)
			.map_err(|e| RipdocError::InvalidTarget(format!("Failed to unpack '{}' to '{}': {e}", rel.display(), out.display())))?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

	#[test]
	fn named_offline_requires_version_when_uncached() {
		let _guard = ENV_LOCK.lock().expect("lock");
		let tmp = tempfile::tempdir().expect("tempdir");
		let original = std::env::var_os("CARGO_HOME");

		unsafe {
			std::env::set_var("CARGO_HOME", tmp.path());
		}

		let err = resolve_source("definitely-not-a-real-crate", true).expect_err("expected err");
		assert!(err.to_string().contains("requires an explicit version"));

		unsafe {
			if let Some(value) = original {
				std::env::set_var("CARGO_HOME", value);
			} else {
				std::env::remove_var("CARGO_HOME");
			}
		}
	}

	#[test]
	fn named_uses_cargo_cache_when_present() {
		let _guard = ENV_LOCK.lock().expect("lock");
		let cargo_home = tempfile::tempdir().expect("tempdir");
		let original = std::env::var_os("CARGO_HOME");
		let index_dir = cargo_home.path().join("registry/src/index.fake");
		let crate_dir = index_dir.join("mini-crate-1.2.3");

		fs::create_dir_all(crate_dir.join("src")).expect("mkdir src");
		fs::write(
			crate_dir.join("Cargo.toml"),
			r#"[package]
name = "mini-crate"
version = "1.2.3"
edition = "2021"
"#,
		)
		.expect("write Cargo.toml");
		fs::write(crate_dir.join("src/lib.rs"), "pub fn hi() {}\n").expect("write lib.rs");

		unsafe {
			std::env::set_var("CARGO_HOME", cargo_home.path());
		}

		let src = resolve_source("mini-crate@1.2.3", true).expect("resolve source");
		assert!(src.entry_file.ends_with("src/lib.rs"));
		assert_eq!(src.crate_name, "mini_crate");

		unsafe {
			if let Some(value) = original {
				std::env::set_var("CARGO_HOME", value);
			} else {
				std::env::remove_var("CARGO_HOME");
			}
		}
	}

	#[test]
	fn extract_strips_prefix_dir() {
		use flate2::Compression;
		use flate2::write::GzEncoder;
		use tar::Builder;

		let temp = tempfile::tempdir().expect("tempdir");
		let src_root = temp.path().join("foo-1.0.0");
		fs::create_dir_all(src_root.join("src")).expect("mkdir src");
		fs::write(
			src_root.join("Cargo.toml"),
			r#"[package]
name = "foo"
version = "1.0.0"
edition = "2021"
"#,
		)
		.expect("write Cargo.toml");
		fs::write(src_root.join("src/lib.rs"), "pub fn x() {}\n").expect("write lib.rs");

		let mut tar_buf = Vec::new();
		{
			let mut builder = Builder::new(&mut tar_buf);
			builder.append_dir_all("foo-1.0.0", &src_root).expect("append tar dir");
			builder.finish().expect("finish tar");
		}

		let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
		encoder.write_all(&tar_buf).expect("write gz input");
		let gz = encoder.finish().expect("finish gz");

		let out = tempfile::tempdir().expect("out dir");
		extract_targz_stripping_prefix(std::io::Cursor::new(gz), out.path()).expect("extract");

		assert!(out.path().join("Cargo.toml").exists());
		assert!(out.path().join("src/lib.rs").exists());
	}
}
