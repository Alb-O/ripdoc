use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use rustdoc_types::Crate;
use semver::Version;

use super::manifest::to_import_name;
use super::path::CargoPath;
use super::registry::fetch_registry_crate;
use crate::error::{Result, RuskelError};
use crate::target::{Entrypoint, Target};

/// A resolved Rust package or module target.
#[derive(Debug)]
pub struct ResolvedTarget {
	/// Package directory path (filesystem or temporary).
	pub(super) package_path: CargoPath,

	/// Module path within the package, excluding the package name. E.g.,
	/// "module::submodule::item". Empty string for package root. This might not necessarily match
	/// the user's input.
	pub filter: String,
}

impl ResolvedTarget {
	/// Build a `ResolvedTarget` with a normalised module filter path.
	pub(super) fn new(path: CargoPath, components: &[String]) -> Self {
		let filter = if components.is_empty() {
			String::new()
		} else {
			let mut normalized_components = components.to_vec();
			normalized_components[0] = to_import_name(&normalized_components[0]);
			normalized_components.join("::")
		};

		Self {
			package_path: path,
			filter,
		}
	}

	/// Read the crate data for this resolved target using rustdoc JSON generation.
	pub fn read_crate(
		&self,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
		silent: bool,
	) -> Result<Crate> {
		self.package_path.read_crate(
			no_default_features,
			all_features,
			features,
			private_items,
			silent,
		)
	}

	/// Resolve a `Target` into a fully-qualified location and filter path.
	pub fn from_target(target: Target, offline: bool) -> Result<Self> {
		match target.entrypoint {
			Entrypoint::Path(path) => {
				if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
					Self::from_rust_file(path, &target.path)
				} else {
					let cargo_path = CargoPath::Path(path.clone());
					if cargo_path.is_package()? {
						Ok(Self::new(cargo_path, &target.path))
					} else if cargo_path.is_workspace()? {
						if target.path.is_empty() {
							// List available packages in the workspace
							let packages = cargo_path.list_workspace_packages()?;
							let mut error_msg =
								"No package specified in workspace.\nAvailable packages:"
									.to_string();
							for package in packages {
								error_msg.push_str(&format!("\n  - {package}"));
							}
							error_msg.push_str("\n\nUsage: ruskel <package-name>");
							Err(RuskelError::InvalidTarget(error_msg))
						} else {
							let package_name = &target.path[0];
							if let Some(package) =
								cargo_path.find_workspace_package(package_name)?
							{
								Ok(Self::new(package.package_path, &target.path[1..]))
							} else {
								Err(RuskelError::ModuleNotFound(format!(
									"Package '{package_name}' not found in workspace"
								)))
							}
						}
					} else {
						Err(RuskelError::InvalidTarget(format!(
							"Path '{}' is neither a package nor a workspace",
							path.display()
						)))
					}
				}
			}
			Entrypoint::Name { name, version } => {
				if let Some(version) = version {
					return Self::from_registry_crate(&name, Some(&version), &target.path, offline);
				}

				let current_dir = env::current_dir()?;
				match CargoPath::nearest_manifest(&current_dir) {
					Some(root) => {
						if let Some(workspace_member) = root.find_workspace_package(&name)? {
							let Self { package_path, .. } = workspace_member;
							return Ok(Self::new(package_path, &target.path));
						}

						if let Some(dependency) = root.find_dependency(&name, offline)? {
							Ok(Self::new(dependency, &target.path))
						} else {
							Self::from_registry_crate(&name, None, &target.path, offline)
						}
					}
					None => Self::from_registry_crate(&name, None, &target.path, offline),
				}
			}
		}
	}

	/// Resolve a module path starting from a specific Rust source file.
	fn from_rust_file(file_path: PathBuf, additional_path: &[String]) -> Result<Self> {
		let file_path = fs::canonicalize(file_path)?;
		let mut current_dir = file_path
			.parent()
			.ok_or_else(|| RuskelError::InvalidTarget("Invalid file path".to_string()))?
			.to_path_buf();

		// Find the nearest Cargo.toml
		while !current_dir.join("Cargo.toml").exists() {
			if !current_dir.pop() {
				return Err(RuskelError::ManifestNotFound);
			}
		}

		let cargo_path = CargoPath::Path(current_dir.clone());
		let relative_path = file_path.strip_prefix(&current_dir).map_err(|_| {
			RuskelError::InvalidTarget("Failed to determine relative path".to_string())
		})?;

		// Convert the relative path to a module path
		let mut components: Vec<_> = relative_path
			.components()
			.filter_map(|c| {
				if let Component::Normal(os_str) = c {
					os_str.to_str().map(String::from)
				} else {
					None
				}
			})
			.collect();

		// Remove "src" if it's the first component
		if components.first().is_some_and(|c| c == "src") {
			components.remove(0);
		}

		// Remove the last component (file name) and add it back without the extension
		if let Some(file_name) = components.pop()
			&& let Some(stem) = Path::new(&file_name).file_stem().and_then(|s| s.to_str())
		{
			components.push(stem.to_string());
		}

		// Combine the module path with the additional path
		components.extend_from_slice(additional_path);

		Ok(Self::new(cargo_path, &components))
	}

	/// Create a resolved target backed by a cached download from crates.io.
	fn from_registry_crate(
		name: &str,
		version: Option<&Version>,
		path: &[String],
		offline: bool,
	) -> Result<Self> {
		let cargo_path = fetch_registry_crate(name, version, offline)?;
		Ok(Self::new(cargo_path, path))
	}
}

/// Resovles a target specification and returns a ResolvedTarget, pointing to the package
/// directory. If necessary, construct temporary dummy crate to download packages from cargo.io.
/// Parse a textual target specification into a `ResolvedTarget`.
pub fn resolve_target(target_str: &str, offline: bool) -> Result<ResolvedTarget> {
	let target = Target::parse(target_str)?;

	match &target.entrypoint {
		Entrypoint::Path(_) => ResolvedTarget::from_target(target, offline),
		Entrypoint::Name {
			name: _,
			version: _,
		} => {
			let resolved = ResolvedTarget::from_target(target.clone(), offline)?;
			if !resolved.filter.is_empty() {
				let first_component = resolved.filter.split("::").next().unwrap().to_string();
				if let Some(cp) = resolved
					.package_path
					.find_dependency(&first_component, offline)?
				{
					Ok(ResolvedTarget::new(cp, &target.path))
				} else {
					Ok(resolved)
				}
			} else {
				Ok(resolved)
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use tempfile::TempDir;

	use super::*;

	enum ExpectedResult {
		Path(PathBuf),
	}

	fn setup_test_structure() -> TempDir {
		let temp_dir = TempDir::new().unwrap();
		let root = temp_dir.path();

		// Create workspace structure
		fs::create_dir_all(root.join("workspace/pkg1/src")).unwrap();
		fs::create_dir_all(root.join("workspace/pkg2/src")).unwrap();
		fs::write(
			root.join("workspace/Cargo.toml"),
			r#"
            [workspace]
            members = ["pkg1", "pkg2"]
            "#,
		)
		.unwrap();

		// Create pkg1
		fs::write(
			root.join("workspace/pkg1/Cargo.toml"),
			r#"
            [package]
            name = "pkg1"
            version = "0.1.0"
            "#,
		)
		.unwrap();
		fs::write(root.join("workspace/pkg1/src/lib.rs"), "// pkg1 lib").unwrap();
		fs::write(root.join("workspace/pkg1/src/module.rs"), "// pkg1 module").unwrap();

		// Create pkg2
		fs::write(
			root.join("workspace/pkg2/Cargo.toml"),
			r#"
            [package]
            name = "pkg2"
            version = "0.1.0"
            [dependencies]
            "#,
		)
		.unwrap();
		fs::write(root.join("workspace/pkg2/src/lib.rs"), "// pkg2 lib").unwrap();

		// Create standalone package
		fs::create_dir_all(root.join("standalone/src")).unwrap();
		fs::write(
			root.join("standalone/Cargo.toml"),
			r#"
            [package]
            name = "standalone"
            version = "0.1.0"
            "#,
		)
		.unwrap();
		fs::write(root.join("standalone/src/lib.rs"), "// standalone lib").unwrap();
		fs::write(
			root.join("standalone/src/module.rs"),
			"// standalone module",
		)
		.unwrap();

		temp_dir
	}

	#[test]
	fn test_from_target() {
		let temp_dir = setup_test_structure();
		let root = temp_dir.path();

		let test_cases = vec![
			(
				Target {
					entrypoint: Entrypoint::Path(root.join("workspace/pkg1")),
					path: vec![],
				},
				ExpectedResult::Path(root.join("workspace/pkg1")),
				vec![],
			),
			(
				Target {
					entrypoint: Entrypoint::Path(root.join("workspace/pkg1")),
					path: vec!["module".to_string()],
				},
				ExpectedResult::Path(root.join("workspace/pkg1")),
				vec!["module".to_string()],
			),
			(
				Target {
					entrypoint: Entrypoint::Path(root.join("workspace")),
					path: vec!["pkg2".to_string()],
				},
				ExpectedResult::Path(root.join("workspace/pkg2")),
				vec![],
			),
			(
				Target {
					entrypoint: Entrypoint::Path(root.join("workspace/pkg1/src/module.rs")),
					path: vec![],
				},
				ExpectedResult::Path(root.join("workspace/pkg1")),
				vec!["module".to_string()],
			),
			(
				Target {
					entrypoint: Entrypoint::Path(root.join("standalone")),
					path: vec!["module".to_string()],
				},
				ExpectedResult::Path(root.join("standalone")),
				vec!["module".to_string()],
			),
		];

		for (i, (target, expected_result, expected_filter)) in test_cases.into_iter().enumerate() {
			let result = ResolvedTarget::from_target(target, true);

			match (result, expected_result) {
				(Ok(resolved), ExpectedResult::Path(expected)) => {
					match &resolved.package_path {
						CargoPath::Path(path) => {
							let resolved_path = fs::canonicalize(path).unwrap();
							let expected_path = fs::canonicalize(expected).unwrap();
							assert_eq!(
								resolved_path, expected_path,
								"Test case {} failed: package_path mismatch",
								i
							);
						}
						CargoPath::TempDir(_) => {
							panic!(
								"Test case {i} failed: expected CargoPath::Path, got CargoPath::TempDir"
							);
						}
					}
					assert_eq!(
						resolved.filter,
						expected_filter.join("::"),
						"Test case {} failed: filter mismatch",
						i
					);
				}
				(Err(e), _) => {
					panic!("Test case {i} failed: expected Ok, but got error '{e}'");
				}
			}
		}
	}
}
