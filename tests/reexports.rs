//! Integration tests for resolving re-exported items in path search.

use std::fs;

use ripdoc::Ripdoc;
use ripdoc::core_api::search::{SearchDomain, SearchIndex, SearchOptions};
use tempfile::TempDir;

#[test]
fn path_search_matches_public_reexports() -> Result<(), Box<dyn std::error::Error>> {
	let temp_dir = TempDir::new()?;
	let src_dir = temp_dir.path().join("src");
	fs::create_dir_all(&src_dir)?;

	fs::write(
		temp_dir.path().join("Cargo.toml"),
		r#"
[package]
name = "dummy_crate"
version = "0.1.0"
edition = "2021"
"#,
	)?;

	fs::write(
		src_dir.join("lib.rs"),
		r#"
mod inner {
    pub trait SelectionAccess {
        fn selected(&self) -> usize;
    }
}

pub use inner::SelectionAccess;
"#,
	)?;

	let target = temp_dir.path().to_str().unwrap().to_string();
	let ripdoc = Ripdoc::new().with_offline(true).with_silent(true);
	let crate_data = ripdoc
		.inspect(&target, false, false, Vec::new(), true)?
		.into_iter()
		.next()
		.ok_or("missing crate")?;

	let crate_name = crate_data
		.index
		.get(&crate_data.root)
		.and_then(|root| root.name.clone())
		.ok_or("missing crate name")?;

	let index = SearchIndex::build(&crate_data, true, Some(temp_dir.path()));
	let mut opts = SearchOptions::new(format!("{crate_name}::SelectionAccess"));
	opts.domains = SearchDomain::PATHS;
	let results = index.search(&opts);

	assert!(results.iter().any(
		|r| r.path_string == format!("{crate_name}::SelectionAccess")
			&& matches!(
				r.kind,
				ripdoc::core_api::search::SearchItemKind::Trait
					| ripdoc::core_api::search::SearchItemKind::TraitAlias
					| ripdoc::core_api::search::SearchItemKind::TypeAlias
			)
	));

	Ok(())
}
