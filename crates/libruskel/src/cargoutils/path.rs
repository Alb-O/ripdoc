use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cargo::core::Workspace;
use cargo::ops;
use rustdoc_json::PackageTarget;
use rustdoc_types::Crate;
use tempfile::TempDir;

use super::config::create_quiet_cargo_config;
use crate::error::{Result, RuskelError, convert_cargo_error};

/// A path to a crate. This can be a directory on the filesystem or a temporary directory.
#[derive(Debug)]
pub enum CargoPath {
	/// Filesystem-backed crate directory containing a manifest.
	Path(PathBuf),
	/// Ephemeral crate stored inside a temporary directory when fetching dependencies.
	TempDir(TempDir),
}

impl CargoPath {
	/// Return the root directory tied to this Cargo source.
	pub fn as_path(&self) -> &Path {
		match self {
			Self::Path(path) => path.as_path(),
			Self::TempDir(temp_dir) => temp_dir.path(),
		}
	}

	/// Load rustdoc JSON for the crate represented by this cargo path.
	/// Read the crate data for this resolved target using rustdoc JSON generation.
	pub fn read_crate(
		&self,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
		silent: bool,
	) -> Result<Crate> {
		use std::io;

		let manifest_path = self.manifest_path()?;

		// Determine which target to document (lib or bin)
		let manifest_content = fs::read_to_string(&manifest_path)?;
		let manifest: cargo_toml::Manifest = cargo_toml::Manifest::from_str(&manifest_content)
			.map_err(|e| RuskelError::ManifestParse(e.to_string()))?;

		let package_target = if manifest.lib.is_some() || self.as_path().join("src/lib.rs").exists()
		{
			// Package has a library target
			PackageTarget::Lib
		} else if !manifest.bin.is_empty() {
			// Package has explicit binary targets, use the first one
			let first_bin = &manifest.bin[0];
			PackageTarget::Bin(first_bin.name.clone().unwrap_or_else(|| {
				manifest
					.package
					.as_ref()
					.map(|p| p.name.clone())
					.unwrap_or_else(|| "main".to_string())
			}))
		} else if self.as_path().join("src/main.rs").exists() {
			// Package has default binary structure (src/main.rs)
			PackageTarget::Bin(
				manifest
					.package
					.as_ref()
					.map(|p| p.name.clone())
					.unwrap_or_else(|| "main".to_string()),
			)
		} else {
			// Fallback to Lib (will fail if there's truly no target)
			PackageTarget::Lib
		};

		let mut captured_stdout = Vec::new();
		let mut captured_stderr = Vec::new();

		let mut builder = rustdoc_json::Builder::default();

		// Only set toolchain if rustup is available
		if super::config::is_rustup_available() {
			builder = builder.toolchain("nightly");
		}

		let build_result = builder
			.manifest_path(manifest_path)
			.package_target(package_target)
			.document_private_items(private_items)
			.no_default_features(no_default_features)
			.all_features(all_features)
			.features(features)
			.quiet(silent)
			.silent(false)
			.build_with_captured_output(&mut captured_stdout, &mut captured_stderr);

		if !silent {
			if !captured_stdout.is_empty() && io::stdout().write_all(&captured_stdout).is_err() {
				// Best-effort output mirroring; ignore write failures.
			}
			if !captured_stderr.is_empty() && io::stderr().write_all(&captured_stderr).is_err() {
				// Best-effort output mirroring; ignore write failures.
			}
		}

		let json_path = build_result.map_err(|err| {
			super::rustdoc_error::map_rustdoc_build_error(&err, &captured_stderr, silent)
		})?;
		let json_content = fs::read_to_string(&json_path)?;
		let crate_data: Crate = serde_json::from_str(&json_content).map_err(|e| {
            let update_msg = if super::config::is_rustup_available() {
                "try running 'rustup update nightly'"
            } else {
                "try updating your nightly Rust toolchain"
            };
            RuskelError::Generate(format!(
                "Failed to parse rustdoc JSON, which may indicate an outdated nightly toolchain - {update_msg}:\nError: {e}"
            ))
        })?;
		Ok(crate_data)
	}

	/// Compute the absolute `Cargo.toml` path for this source.
	pub fn manifest_path(&self) -> Result<PathBuf> {
		use std::path::absolute;
		let manifest_path = self.as_path().join("Cargo.toml");
		absolute(&manifest_path).map_err(|err| {
			RuskelError::Generate(format!(
				"Failed to resolve manifest path for '{}': {err}",
				manifest_path.display()
			))
		})
	}

	/// Return whether this cargo path includes a `Cargo.toml`.
	pub fn has_manifest(&self) -> Result<bool> {
		Ok(self.as_path().join("Cargo.toml").exists())
	}

	/// Identify if the path is a standalone package manifest.
	pub fn is_package(&self) -> Result<bool> {
		Ok(self.has_manifest()? && !self.is_workspace()?)
	}

	/// Identify if the path is a workspace manifest without a package section.
	pub fn is_workspace(&self) -> Result<bool> {
		if !self.has_manifest()? {
			return Ok(false);
		}
		let manifest_path = self.manifest_path()?;
		let manifest = cargo_toml::Manifest::from_path(&manifest_path)
			.map_err(|err| RuskelError::ManifestParse(err.to_string()))?;
		Ok(manifest.workspace.is_some() && manifest.package.is_none())
	}

	/// Find a dependency within the current workspace or registry cache.
	pub fn find_dependency(&self, dependency: &str, offline: bool) -> Result<Option<Self>> {
		let config = create_quiet_cargo_config(offline)?;
		let manifest_path = self.manifest_path()?;

		let workspace =
			Workspace::new(&manifest_path, &config).map_err(|err| convert_cargo_error(&err))?;

		let (_, ps) = ops::fetch(
			&workspace,
			&ops::FetchOptions {
				gctx: &config,
				targets: vec![],
			},
		)
		.map_err(|err| convert_cargo_error(&err))?;

		// Try both the provided name and its hyphenated/underscored version
		let alt_dependency = if dependency.contains('_') {
			dependency.replace('_', "-")
		} else {
			dependency.replace('-', "_")
		};

		for package in ps.packages() {
			let package_name = package.name().as_str();
			if package_name == dependency || package_name == alt_dependency {
				return Ok(Some(Self::Path(
					package.manifest_path().parent().unwrap().to_path_buf(),
				)));
			}
		}
		Ok(None)
	}

	/// Walk upwards from `start_dir` to locate the closest `Cargo.toml`.
	pub fn nearest_manifest(start_dir: &Path) -> Option<Self> {
		let mut current_dir = start_dir.to_path_buf();

		loop {
			let manifest_path = current_dir.join("Cargo.toml");
			if manifest_path.exists() {
				return Some(Self::Path(current_dir));
			}
			if !current_dir.pop() {
				break;
			}
		}
		None
	}

	/// Find a package in the current workspace by name.
	pub(super) fn find_workspace_package(
		&self,
		module_name: &str,
	) -> Result<Option<super::resolved_target::ResolvedTarget>> {
		let workspace_manifest_path = self.manifest_path()?;

		// Try both hyphenated and underscored versions
		let alt_name = if module_name.contains('_') {
			module_name.replace('_', "-")
		} else {
			module_name.replace('-', "_")
		};

		let config = create_quiet_cargo_config(false)?;

		let workspace = Workspace::new(&workspace_manifest_path, &config)
			.map_err(|err| convert_cargo_error(&err))?;

		for package in workspace.members() {
			let package_name = package.name().as_str();
			if package_name == module_name || package_name == alt_name {
				let package_path = package.manifest_path().parent().unwrap().to_path_buf();
				return Ok(Some(super::resolved_target::ResolvedTarget::new(
					Self::Path(package_path),
					&[],
				)));
			}
		}
		Ok(None)
	}

	/// List all packages in the current workspace.
	pub(super) fn list_workspace_packages(&self) -> Result<Vec<String>> {
		let workspace_manifest_path = self.manifest_path()?;
		let config = create_quiet_cargo_config(false)?;

		let workspace = Workspace::new(&workspace_manifest_path, &config)
			.map_err(|err| convert_cargo_error(&err))?;

		let mut packages = Vec::new();
		for package in workspace.members() {
			packages.push(package.name().as_str().to_string());
		}

		packages.sort();
		Ok(packages)
	}
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;

	use super::*;

	#[test]
	fn test_is_workspace() -> Result<()> {
		let temp_dir = tempdir()?;
		let cargo_path = CargoPath::Path(temp_dir.path().to_path_buf());

		// Create a workspace Cargo.toml
		let manifest = r#"
            [workspace]
            members = ["member1", "member2"]
        "#;
		let manifest_path = cargo_path.manifest_path()?;
		fs::write(&manifest_path, manifest)?;
		assert!(cargo_path.is_workspace()?);

		// Create a regular Cargo.toml
		fs::write(
			&manifest_path,
			r#"
[package]
name = "test-crate"
version = "0.1.0"
"#,
		)?;
		assert!(!cargo_path.is_workspace()?);

		Ok(())
	}
}
