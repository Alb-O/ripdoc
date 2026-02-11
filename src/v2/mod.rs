//! v2 backend (tree-sitter / source-only).
//!
//! Slice 1: only `list` is routed here behind `--features v2-ts` +
//! `RIPDOC_BACKEND=ts`.

#![cfg(feature = "v2-ts")]

mod parse;
mod scan;
mod search;
mod source;

use crate::core_api::{ListItem, Result, Ripdoc, SearchOptions};

/// v2 list entrypoint called from `Ripdoc::list` when `RIPDOC_BACKEND=ts`.
pub(crate) fn list_v2(
	_rs: &Ripdoc,
	target: &str,
	include_private: bool,
	search_opts: Option<&SearchOptions>,
) -> Result<Vec<ListItem>> {
	let src = source::resolve_local_source(target)?;
	let mut items = scan::list_crate(&src, include_private)?;
	if let Some(opts) = search_opts {
		items = search::filter_list(items, opts);
	}
	Ok(items)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use crate::{SearchDomain, SearchItemKind, SearchOptions};

	use super::*;

	#[test]
	fn list_v2_module_graph_external_mods() {
		let dir = tempfile::tempdir().expect("tempdir");
		fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
		fs::write(
			dir.path().join("Cargo.toml"),
			r#"[package]
name = "mini-crate"
version = "0.1.0"
edition = "2021"
"#,
		)
		.expect("write Cargo.toml");

		fs::write(
			dir.path().join("src/lib.rs"),
			r#"
pub mod foo;
pub mod inline {
    pub fn a() {}
    mod deep {
        pub fn hidden() {}
    }
}
fn hidden_root() {}
"#,
		)
		.expect("write lib.rs");

		fs::write(
			dir.path().join("src/foo.rs"),
			r#"
pub fn bar() {}
pub mod sub;
"#,
		)
		.expect("write foo.rs");

		fs::create_dir_all(dir.path().join("src/foo")).expect("mkdir src/foo");
		fs::write(
			dir.path().join("src/foo/sub.rs"),
			r#"
pub fn qux() {}
"#,
		)
		.expect("write sub.rs");

		let rs = Ripdoc::new().with_offline(true);
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None)
			.expect("list_v2");

		assert!(items.iter().any(|i| i.kind == SearchItemKind::Crate && i.path == "mini_crate"));
		assert!(
			items
				.iter()
				.any(|i| i.kind == SearchItemKind::Module && i.path == "mini_crate::foo")
		);
		assert!(items.iter().any(|i| {
			i.kind == SearchItemKind::Module && i.path == "mini_crate::foo::sub"
		}));
		assert!(items.iter().any(|i| {
			i.kind == SearchItemKind::Function && i.path == "mini_crate::foo::bar"
		}));
		assert!(items.iter().any(|i| {
			i.kind == SearchItemKind::Function && i.path == "mini_crate::foo::sub::qux"
		}));
		assert!(items.iter().any(|i| {
			i.kind == SearchItemKind::Module && i.path == "mini_crate::inline"
		}));
		assert!(items.iter().any(|i| {
			i.kind == SearchItemKind::Function && i.path == "mini_crate::inline::a"
		}));
		assert!(!items.iter().any(|i| i.path.ends_with("hidden_root")));
		assert!(!items.iter().any(|i| i.path.contains("deep")));
		assert!(!items.iter().any(|i| i.path.ends_with("hidden")));
	}

	#[test]
	fn list_v2_search_name_and_path() {
		let dir = tempfile::tempdir().expect("tempdir");
		fs::create_dir_all(dir.path().join("src/foo")).expect("mkdir src/foo");
		fs::write(
			dir.path().join("Cargo.toml"),
			r#"[package]
name = "mini-crate"
version = "0.1.0"
edition = "2021"
"#,
		)
		.expect("write Cargo.toml");
		fs::write(dir.path().join("src/lib.rs"), "pub mod foo;\n").expect("write lib.rs");
		fs::write(dir.path().join("src/foo.rs"), "pub mod sub;\n").expect("write foo.rs");
		fs::write(dir.path().join("src/foo/sub.rs"), "pub fn qux() {}\n")
			.expect("write sub.rs");

		let rs = Ripdoc::new().with_offline(true);

		let mut opts = SearchOptions::new("qux");
		opts.domains = SearchDomain::NAMES;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts))
			.expect("list_v2");
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub::qux"));
		assert!(!items.iter().any(|i| i.path == "mini_crate"));

		let mut opts = SearchOptions::new("foo::sub");
		opts.domains = SearchDomain::PATHS;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts))
			.expect("list_v2");
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub"));
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub::qux"));
	}
}
