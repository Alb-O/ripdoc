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
}
