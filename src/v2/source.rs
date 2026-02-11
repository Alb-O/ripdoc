#![cfg(feature = "v2-ts")]

use std::path::{Path, PathBuf};

use crate::cargo_utils::target::{Entrypoint, Target};
use crate::cargo_utils::to_import_name;
use crate::core_api::error::RipdocError;
use crate::core_api::Result;

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

/// Resolve a local crate source for v2 list.
///
/// Slice 1 / Step 2 supports local path targets and only `src/lib.rs` as entrypoint.
pub(crate) fn resolve_local_source(target: &str) -> Result<V2Source> {
	let parsed = Target::parse(target)?;
	let Entrypoint::Path(path) = parsed.entrypoint else {
		return Err(RipdocError::InvalidTarget(
			"v2 list currently only supports local path targets (directory/file path)"
				.to_string(),
		));
	};

	let abs = if path.is_relative() {
		std::path::absolute(&path).map_err(|e| {
			RipdocError::InvalidTarget(format!(
				"Failed to resolve target path '{}': {e}",
				path.display()
			))
		})?
	} else {
		path
	};

	let root_dir = if abs.is_file() {
		let start = abs.parent().unwrap_or(abs.as_path());
		find_manifest_root(start)?
	} else {
		find_manifest_root(&abs)?
	};

	let manifest_path = root_dir.join("Cargo.toml");
	if !manifest_path.exists() {
		return Err(RipdocError::InvalidTarget(format!(
			"No Cargo.toml found at or above '{}'",
			abs.display()
		)));
	}

	let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| {
		RipdocError::InvalidTarget(format!(
			"Failed to read manifest '{}': {e}",
			manifest_path.display()
		))
	})?;
	let manifest: cargo_toml::Manifest = cargo_toml::Manifest::from_str(&manifest_str)
		.map_err(|e| {
			RipdocError::InvalidTarget(format!(
				"Failed to parse manifest '{}': {e}",
				manifest_path.display()
			))
		})?;

	let Some(pkg) = manifest.package.as_ref() else {
		return Err(RipdocError::InvalidTarget(format!(
			"Workspace/virtual manifest not supported yet by v2 list: {}",
			manifest_path.display()
		)));
	};

	let package_name = pkg.name.clone();
	let crate_name = to_import_name(&package_name);

	let entry_file = root_dir.join("src").join("lib.rs");
	if !entry_file.exists() {
		return Err(RipdocError::InvalidTarget(format!(
			"v2 list (checkpoint) expected '{}' to exist.",
			entry_file.display()
		)));
	}

	let root_dir = root_dir.canonicalize().unwrap_or(root_dir);
	let entry_file = entry_file.canonicalize().unwrap_or(entry_file);
	let entry_rel_path = entry_file
		.strip_prefix(&root_dir)
		.unwrap_or(entry_file.as_path())
		.to_path_buf();

	let source_prefix = root_dir
		.file_name()
		.and_then(|os| os.to_str())
		.map(str::to_string)
		.unwrap_or_else(|| crate_name.clone());

	Ok(V2Source {
		root_dir,
		package_name,
		crate_name,
		entry_file,
		entry_rel_path,
		source_prefix,
	})
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
