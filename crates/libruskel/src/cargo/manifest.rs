/// Construct a minimal manifest string for a temporary crate that depends on `dependency`.
pub fn generate_dummy_manifest(
	dependency: &str,
	version: Option<String>,
	features: Option<&[&str]>,
) -> String {
	// Convert underscores to hyphens for Cargo package names
	let cargo_dependency = dependency.replace('_', "-");

	let version_str = version.map_or("*".to_string(), |v| v);
	let features_str = features.map_or(String::new(), |f| {
		let feature_list = f
			.iter()
			.map(|feat| format!("\"{feat}\""))
			.collect::<Vec<_>>()
			.join(", ");
		format!(", features = [{feature_list}]")
	});
	format!(
		r#"[package]
name = "dummy-crate"
version = "0.1.0"

[dependencies]
{cargo_dependency} = {{ version = "{version_str}"{features_str} }}
"#
	)
}

/// Convert a package name into its canonical import form by replacing hyphens.
pub fn to_import_name(package_name: &str) -> String {
	package_name.replace('-', "_")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_to_import_name() {
		assert_eq!(to_import_name("serde"), "serde");
		assert_eq!(to_import_name("serde-json"), "serde_json");
		assert_eq!(to_import_name("tokio-util"), "tokio_util");
		assert_eq!(
			to_import_name("my-hyphenated-package"),
			"my_hyphenated_package"
		);
	}

	#[test]
	fn test_generate_dummy_manifest() {
		// Test without features
		let manifest = generate_dummy_manifest("serde", None, None);
		assert!(manifest.contains("serde = { version = \"*\" }"));
		assert!(!manifest.contains("features"));

		// Test with single feature
		let manifest = generate_dummy_manifest("tokio", Some("1.0".to_string()), Some(&["rt"]));
		assert!(manifest.contains("tokio = { version = \"1.0\", features = [\"rt\"] }"));

		// Test with multiple features
		let manifest = generate_dummy_manifest("tokio", None, Some(&["rt", "macros", "test-util"]));
		assert!(manifest.contains(
			"tokio = { version = \"*\", features = [\"rt\", \"macros\", \"test-util\"] }"
		));

		// Validate TOML syntax by parsing
		let manifest = generate_dummy_manifest("serde", None, Some(&["derive", "std"]));
		// Just verify the manifest contains the expected strings, since we don't have toml crate in tests
		assert!(manifest.contains("[dependencies]"));
		assert!(manifest.contains("serde = { version = \"*\", features = [\"derive\", \"std\"] }"));
	}

	#[test]
	fn test_generate_dummy_manifest_with_underscores() {
		// Test underscore to hyphen conversion
		let manifest = generate_dummy_manifest("serde_json", None, None);
		assert!(manifest.contains("serde-json = { version = \"*\" }"));
		assert!(!manifest.contains("serde_json"));

		// Test with already hyphenated names (should remain unchanged)
		let manifest = generate_dummy_manifest("async-trait", None, None);
		assert!(manifest.contains("async-trait = { version = \"*\" }"));

		// Test complex name with multiple underscores
		let manifest =
			generate_dummy_manifest("my_complex_crate_name", Some("0.1.0".to_string()), None);
		assert!(manifest.contains("my-complex-crate-name = { version = \"0.1.0\" }"));
	}
}
