use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use rustdoc_types::Crate;
use semver::Version;

use super::error::{Result, RipdocError};
use super::path::CargoPath;
use super::registry::fetch_registry_crate;
use super::target::{Entrypoint, Target};
use super::to_import_name;

/// A resolved Rust package or module target.
#[derive(Debug)]
pub struct ResolvedTarget {
	/// Package directory path (filesystem or temporary).
	pub(super) package_path: CargoPath,

	/// Module path within the package, excluding the package name. E.g.,
	/// "module::submodule::item". Empty string for package root. This might not necessarily match
	/// the user's input.
	pub filter: String,

	/// The name of the package.
	pub package_name: Option<String>,
}

enum TargetResolution {
	FileModule {
		file: PathBuf,
		extra_path: Vec<String>,
	},
	PackageDir {
		package: CargoPath,
		extra_path: Vec<String>,
	},
	WorkspaceRoot {
		workspace: CargoPath,
		extra_path: Vec<String>,
	},
	NamedCrate {
		name: String,
		version: Option<Version>,
		extra_path: Vec<String>,
	},
}

impl TargetResolution {
	fn plan(target: Target) -> Result<Self> {
		match target.entrypoint {
			Entrypoint::Path(path) => {
				let path = if path.is_relative() {
					std::path::absolute(&path).map_err(|err| {
						RipdocError::InvalidTarget(format!(
							"Failed to resolve target path '{}': {err}",
							path.display()
						))
					})?
				} else {
					path
				};

				if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
					return Ok(Self::FileModule {
						file: path,
						extra_path: target.path,
					});
				}

				let cargo_path = CargoPath::Path(path.clone());
				if cargo_path.is_package()? {
					Ok(Self::PackageDir {
						package: cargo_path,
						extra_path: target.path,
					})
				} else if cargo_path.is_workspace()? {
					Ok(Self::WorkspaceRoot {
						workspace: cargo_path,
						extra_path: target.path,
					})
				} else {
					Err(RipdocError::InvalidTarget(format!(
						"Path '{}' is neither a package nor a workspace",
						path.display()
					)))
				}
			}
			Entrypoint::Name { name, version } => Ok(Self::NamedCrate {
				name,
				version,
				extra_path: target.path,
			}),
		}
	}

	fn resolve(self, offline: bool) -> Result<Vec<ResolvedTarget>> {
		match self {
			Self::FileModule { file, extra_path } => {
				Ok(vec![ResolvedTarget::from_rust_file(file, &extra_path)?])
			}
			Self::PackageDir {
				package,
				extra_path,
			} => Ok(vec![ResolvedTarget::new(package, &extra_path, None)]),
			Self::WorkspaceRoot {
				workspace,
				mut extra_path,
			} => {
				if extra_path.is_empty() {
					let packages = workspace.list_workspace_packages()?;
					return Ok(packages
						.into_iter()
						.map(|(name, path)| {
							ResolvedTarget::new(CargoPath::Path(path), &[], Some(name))
						})
						.collect());
				}
				let package_name = extra_path.remove(0);
				if let Some(package) = workspace.find_workspace_package(&package_name)? {
					Ok(vec![ResolvedTarget::new(
						package.package_path,
						&extra_path,
						Some(package_name),
					)])
				} else {
					Err(RipdocError::ModuleNotFound(format!(
						"Package '{package_name}' not found in workspace"
					)))
				}
			}
			Self::NamedCrate {
				name,
				version,
				extra_path,
			} => Ok(vec![ResolvedTarget::resolve_named_target(
				&name,
				version.as_ref(),
				&extra_path,
				offline,
			)?]),
		}
	}
}

impl ResolvedTarget {
	/// Build a `ResolvedTarget` with a normalised module filter path.
	pub(super) fn new(
		path: CargoPath,
		components: &[String],
		package_name: Option<String>,
	) -> Self {
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
			package_name,
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
		cache_config: &super::cache::CacheConfig,
	) -> Result<Crate> {
		self.package_path.read_crate(
			no_default_features,
			all_features,
			features,
			private_items,
			silent,
			cache_config,
		)
	}

	/// Return the crate root on disk backing this target.
	pub fn package_root(&self) -> &Path {
		self.package_path.as_path()
	}

	/// Resolve a `Target` into a fully-qualified location and filter path.
	pub fn from_target(target: Target, offline: bool) -> Result<Vec<Self>> {
		let resolution = TargetResolution::plan(target)?;
		resolution.resolve(offline)
	}

	/// Resolve a module path starting from a specific Rust source file.
	fn from_rust_file(file_path: PathBuf, additional_path: &[String]) -> Result<Self> {
		let file_path = fs::canonicalize(file_path)?;
		let mut current_dir = file_path
			.parent()
			.ok_or_else(|| RipdocError::InvalidTarget("Invalid file path".to_string()))?
			.to_path_buf();

		// Find the nearest Cargo.toml
		while !current_dir.join("Cargo.toml").exists() {
			if !current_dir.pop() {
				return Err(RipdocError::ManifestNotFound);
			}
		}

		let cargo_path = CargoPath::Path(current_dir.clone());
		let relative_path = file_path.strip_prefix(&current_dir).map_err(|_| {
			RipdocError::InvalidTarget("Failed to determine relative path".to_string())
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

		Ok(Self::new(cargo_path, &components, None))
	}

	/// Create a resolved target backed by a cached download from crates.io.
	fn from_registry_crate(
		name: &str,
		version: Option<&Version>,
		path: &[String],
		offline: bool,
	) -> Result<Self> {
		let cargo_path = fetch_registry_crate(name, version, offline)?;
		Ok(Self::new(cargo_path, path, Some(name.to_string())))
	}

	fn resolve_named_target(
		name: &str,
		version: Option<&Version>,
		path: &[String],
		offline: bool,
	) -> Result<Self> {
		if let Some(version) = version {
			return Self::from_registry_crate(name, Some(version), path, offline);
		}

		let current_dir = env::current_dir()?;
		if let Some(root) = CargoPath::nearest_manifest(&current_dir) {
			if let Some(workspace_member) = root.find_workspace_package(name)? {
				return Ok(Self::new(
					workspace_member.package_path,
					path,
					Some(name.to_string()),
				));
			}

			if let Some(dependency) = root.find_dependency(name, offline)? {
				return Ok(Self::new(dependency, path, Some(name.to_string())));
			}
		}

		Self::from_registry_crate(name, None, path, offline)
	}
}

/// Resovles a target specification and returns a ResolvedTarget, pointing to the package
/// directory. If necessary, construct temporary dummy crate to download packages from cargo.io.
/// Parse a textual target specification into a `ResolvedTarget`.
pub fn resolve_target(target_str: &str, offline: bool) -> Result<Vec<ResolvedTarget>> {
	let target = Target::parse(target_str)?;

	match &target.entrypoint {
		Entrypoint::Path(_) => ResolvedTarget::from_target(target, offline),
		Entrypoint::Name { name, version } => {
			let resolved_list = ResolvedTarget::resolve_named_target(
				name,
				version.as_ref(),
				&target.path,
				offline,
			)?;
			Ok(vec![resolved_list])
		}
	}
}

#[cfg(test)]
mod tests {
	use std::env;
	use std::path::{Path, PathBuf};
	use std::sync::{Mutex, MutexGuard};

	use once_cell::sync::Lazy;
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
            standalone = { path = "../../standalone" }
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
		fs::create_dir_all(root.join("external")).unwrap();

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
				(Ok(resolved_list), ExpectedResult::Path(expected)) => {
					let resolved = &resolved_list[0];
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

	static DIR_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

	struct DirGuard {
		original: PathBuf,
		_lock: MutexGuard<'static, ()>,
	}

	impl DirGuard {
		fn change_to(path: &Path) -> Self {
			let lock = DIR_MUTEX.lock().unwrap();
			let original = env::current_dir().unwrap();
			env::set_current_dir(path).unwrap();
			Self {
				original,
				_lock: lock,
			}
		}
	}

	impl Drop for DirGuard {
		fn drop(&mut self) {
			let _ = env::set_current_dir(&self.original);
		}
	}

	#[test]
	fn named_target_prefers_workspace_member() {
		let temp_dir = setup_test_structure();
		let root = temp_dir.path();
		let _guard = DirGuard::change_to(&root.join("workspace"));
		let target = Target {
			entrypoint: Entrypoint::Name {
				name: "pkg1".to_string(),
				version: None,
			},
			path: vec![],
		};

		let resolved_list = ResolvedTarget::from_target(target, true).expect("workspace member");
		let resolved = &resolved_list[0];
		match &resolved.package_path {
			CargoPath::Path(path) => {
				assert_eq!(
					fs::canonicalize(path).unwrap(),
					fs::canonicalize(root.join("workspace/pkg1")).unwrap()
				);
			}
			_ => panic!("expected workspace member to be filesystem path"),
		}
	}

	#[test]
	fn named_target_prefers_dependency() {
		let temp_dir = setup_test_structure();
		let root = temp_dir.path();
		let _guard = DirGuard::change_to(&root.join("workspace/pkg2"));

		let target = Target {
			entrypoint: Entrypoint::Name {
				name: "standalone".to_string(),
				version: None,
			},
			path: vec![],
		};

		let resolved_list = ResolvedTarget::from_target(target, true).expect("dependency");
		let resolved = &resolved_list[0];
		match &resolved.package_path {
			CargoPath::Path(path) => {
				assert_eq!(
					fs::canonicalize(path).unwrap(),
					fs::canonicalize(root.join("standalone")).unwrap()
				);
			}
			_ => panic!("expected dependency to resolve to filesystem path"),
		}
	}

	#[test]
	fn registry_target_requires_version_offline() {
		let temp_dir = setup_test_structure();
		let root = temp_dir.path();
		let _guard = DirGuard::change_to(&root.join("external"));

		let target = Target {
			entrypoint: Entrypoint::Name {
				name: "nonexistent-crate-for-test".to_string(),
				version: None,
			},
			path: vec![],
		};

		let err = ResolvedTarget::from_target(target, true).unwrap_err();
		assert!(
			err.to_string().contains("requires an explicit version"),
			"unexpected error: {err}"
		);
	}
}
