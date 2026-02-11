//! v2 backend (tree-sitter / source-only).
//!
//! Slice 1: only `list` is routed here behind `--features v2-ts` +
//! `RIPDOC_BACKEND=ts`.

#![cfg(feature = "v2-ts")]

mod entry;
mod parse;
mod render;
mod scan;
mod search;
mod source;

use entry::V2Entry;

use crate::cargo_utils::target::Target;
use crate::core_api::{ListItem, Result, Ripdoc, SearchItemKind, SearchOptions, SearchResponse};

/// v2 list entrypoint called from `Ripdoc::list` when `RIPDOC_BACKEND=ts`.
pub(crate) fn list_v2(rs: &Ripdoc, target: &str, include_private: bool, search_opts: Option<&SearchOptions>) -> Result<Vec<ListItem>> {
	let parsed = Target::parse(target)?;
	let target_path = parsed.path.clone();
	let src = source::resolve_source(target, rs.offline())?;
	let mut entries: Vec<V2Entry> = scan::list_crate(&src, include_private)?;
	if !target_path.is_empty() {
		entries = filter_by_target_path(entries, &src.crate_name, &target_path);
	}
	if let Some(opts) = search_opts {
		entries = search::filter_entries(entries, opts);
	}

	let mut items: Vec<ListItem> = entries.into_iter().map(|entry| entry.to_list_item()).collect();
	items.retain(|item| item.kind != SearchItemKind::Use);
	Ok(items)
}

/// v2 render entrypoint called from `Ripdoc::render` when `RIPDOC_BACKEND=ts`.
pub(crate) fn render_v2(rs: &Ripdoc, target: &str, private_items: bool) -> Result<String> {
	let parsed = Target::parse(target)?;
	let target_path = parsed.path.clone();
	let src = source::resolve_source(target, rs.offline())?;
	let mut entries: Vec<V2Entry> = scan::list_crate(&src, private_items)?;
	if !target_path.is_empty() {
		entries = filter_by_target_path(entries, &src.crate_name, &target_path);
	}
	entries.retain(|entry| entry.kind != SearchItemKind::Use);

	Ok(render::render_entries(
		rs.render_format(),
		rs.render_source_labels(),
		&src.package_name,
		&src.crate_name,
		&entries,
	))
}

/// v2 search entrypoint called from `Ripdoc::search` when `RIPDOC_BACKEND=ts`.
pub(crate) fn search_v2(rs: &Ripdoc, target: &str, options: &SearchOptions) -> Result<SearchResponse> {
	let parsed = Target::parse(target)?;
	let target_path = parsed.path.clone();
	let src = source::resolve_source(target, rs.offline())?;
	let mut entries: Vec<V2Entry> = scan::list_crate(&src, options.include_private)?;
	if !target_path.is_empty() {
		entries = filter_by_target_path(entries, &src.crate_name, &target_path);
	}
	let all_entries = entries.clone();
	let mut filtered = search::filter_entries(entries, options);
	if options.expand_containers {
		filtered = include_ancestor_modules(&all_entries, filtered, &src.crate_name);
	}
	filtered.retain(|entry| entry.kind != SearchItemKind::Use);

	let rendered = render::render_entries(rs.render_format(), rs.render_source_labels(), &src.package_name, &src.crate_name, &filtered);

	Ok(SearchResponse { results: Vec::new(), rendered })
}

fn normalize_filter_segments(crate_name: &str, segments: &[String]) -> Vec<String> {
	let mut out: Vec<String> = segments.to_vec();
	while let Some(first) = out.first().map(|s| s.as_str()) {
		if first == "crate" || first == crate_name {
			out.remove(0);
		} else {
			break;
		}
	}
	out
}

fn filter_by_target_path(entries: Vec<V2Entry>, crate_name: &str, segments: &[String]) -> Vec<V2Entry> {
	let segments = normalize_filter_segments(crate_name, segments);
	if segments.is_empty() {
		return entries;
	}

	let prefix = format!("{crate_name}::{}", segments.join("::"));
	let prefix_with_delim = format!("{prefix}::");

	entries
		.into_iter()
		.filter(|entry| entry.kind == SearchItemKind::Crate || entry.path == prefix || entry.path.starts_with(&prefix_with_delim))
		.collect()
}

fn include_ancestor_modules(all_entries: &[V2Entry], mut matches: Vec<V2Entry>, crate_name: &str) -> Vec<V2Entry> {
	use std::collections::{HashMap, HashSet};

	let mut modules_by_path: HashMap<&str, &V2Entry> = HashMap::new();
	for entry in all_entries {
		if entry.kind == SearchItemKind::Module {
			modules_by_path.insert(entry.path.as_str(), entry);
		}
	}

	let mut needed: HashSet<String> = HashSet::new();
	for item in &matches {
		let segments: Vec<&str> = item.path.split("::").collect();
		if segments.first().copied() != Some(crate_name) {
			continue;
		}
		for i in 2..segments.len() {
			needed.insert(segments[..i].join("::"));
		}
	}

	let mut out_by_path: HashMap<String, V2Entry> = HashMap::new();
	for item in matches.drain(..) {
		out_by_path.insert(item.path.clone(), item);
	}

	for path in needed {
		if out_by_path.contains_key(&path) {
			continue;
		}
		if let Some(module_entry) = modules_by_path.get(path.as_str()) {
			out_by_path.insert(path, (*module_entry).clone());
		}
	}

	let mut out: Vec<V2Entry> = out_by_path.into_values().collect();
	out.sort_by(|a, b| a.path.cmp(&b.path));
	out
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
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2");

		assert!(items.iter().any(|i| i.kind == SearchItemKind::Crate && i.path == "mini_crate"));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Module && i.path == "mini_crate::foo"));
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::Module && i.path == "mini_crate::foo::sub" }));
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::Function && i.path == "mini_crate::foo::bar" }));
		assert!(
			items
				.iter()
				.any(|i| { i.kind == SearchItemKind::Function && i.path == "mini_crate::foo::sub::qux" })
		);
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::Module && i.path == "mini_crate::inline" }));
		assert!(
			items
				.iter()
				.any(|i| { i.kind == SearchItemKind::Function && i.path == "mini_crate::inline::a" })
		);
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
		fs::write(dir.path().join("src/foo/sub.rs"), "pub fn qux() {}\n").expect("write sub.rs");

		let rs = Ripdoc::new().with_offline(true);

		let mut opts = SearchOptions::new("qux");
		opts.domains = SearchDomain::NAMES;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts)).expect("list_v2");
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub::qux"));
		assert!(!items.iter().any(|i| i.path == "mini_crate"));

		let mut opts = SearchOptions::new("foo::sub");
		opts.domains = SearchDomain::PATHS;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts)).expect("list_v2");
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub"));
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::sub::qux"));
	}

	#[test]
	fn list_v2_expanded_kinds_present() {
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
/// Struct docs
pub struct Foo;
/// Enum docs
pub enum E { A }
/// Trait docs
pub trait T {}
/// Alias docs
pub type Bytes = Vec<u8>;
/// Const docs
pub const N: usize = 1;
/// Static docs
pub static S: i32 = 0;
/// Do something
pub fn do_it(x: i32) -> i32 { x }
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2");

		assert!(items.iter().any(|i| i.kind == SearchItemKind::Struct && i.path.ends_with("::Foo")));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Enum && i.path.ends_with("::E")));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Trait && i.path.ends_with("::T")));
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::TypeAlias && i.path.ends_with("::Bytes") }));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Constant && i.path.ends_with("::N")));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Static && i.path.ends_with("::S")));
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::Function && i.path.ends_with("::do_it") }));
	}

	#[test]
	fn list_v2_search_docs() {
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
/// Do something very specific
pub fn do_it(x: i32) -> i32 { x }
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let mut opts = SearchOptions::new("very specific");
		opts.domains = SearchDomain::DOCS;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts)).expect("list_v2");
		assert!(items.iter().any(|i| i.path.ends_with("::do_it")));
	}

	#[test]
	fn list_v2_search_signature() {
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
pub type Bytes = Vec<u8>;
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let mut opts = SearchOptions::new("Vec<u8>");
		opts.domains = SearchDomain::SIGNATURES;
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, Some(&opts)).expect("list_v2");
		assert!(items.iter().any(|i| { i.kind == SearchItemKind::TypeAlias && i.path.ends_with("::Bytes") }));
	}

	#[test]
	fn list_v2_reexports_add_alias_entries_without_use_rows() {
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
pub mod inner {
    pub struct S;
    pub enum E { A }
}

pub use inner::S;
pub use inner::E as EE;
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2");

		assert!(items.iter().any(|i| { i.kind == SearchItemKind::Struct && i.path == "mini_crate::inner::S" }));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Struct && i.path == "mini_crate::S"));
		assert!(items.iter().any(|i| i.kind == SearchItemKind::Enum && i.path == "mini_crate::EE"));
		assert!(!items.iter().any(|i| i.kind == SearchItemKind::Use));
	}

	#[test]
	fn list_v2_macro_export_appears_at_crate_root() {
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
pub mod m {
    #[macro_export]
    macro_rules! mm {
        () => {};
    }
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2");

		assert!(items.iter().any(|i| i.kind == SearchItemKind::Macro && i.path == "mini_crate::mm"));
		assert!(!items.iter().any(|i| i.kind == SearchItemKind::Macro && i.path == "mini_crate::m::mm"));
	}

	#[test]
	fn list_v2_proc_macro_is_tagged() {
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
use proc_macro::TokenStream;

#[proc_macro]
pub fn my_macro(input: TokenStream) -> TokenStream {
    input
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2");

		assert!(
			items
				.iter()
				.any(|i| { i.kind == SearchItemKind::ProcMacro && i.path == "mini_crate::my_macro" })
		);
	}

	#[test]
	fn list_v2_private_module_with_public_reexport() {
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
mod inner {
    pub struct S;
}

pub use inner::S;
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);

		let public_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2 public");
		assert!(public_items.iter().any(|i| i.kind == SearchItemKind::Struct && i.path == "mini_crate::S"));
		assert!(!public_items.iter().any(|i| i.path == "mini_crate::inner"));
		assert!(!public_items.iter().any(|i| i.path == "mini_crate::inner::S"));

		let private_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), true, None).expect("list_v2 private");
		assert!(private_items.iter().any(|i| i.path == "mini_crate::inner"));
		assert!(private_items.iter().any(|i| i.path == "mini_crate::inner::S"));
		assert!(private_items.iter().any(|i| i.path == "mini_crate::S"));
	}

	#[test]
	fn list_v2_pub_crate_items_hidden_unless_private() {
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
pub(crate) fn c() {}
pub fn p() {}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let public_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2 public");
		assert!(public_items.iter().any(|i| i.path == "mini_crate::p"));
		assert!(!public_items.iter().any(|i| i.path == "mini_crate::c"));

		let private_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), true, None).expect("list_v2 private");
		assert!(private_items.iter().any(|i| i.path == "mini_crate::p"));
		assert!(private_items.iter().any(|i| i.path == "mini_crate::c"));
	}

	#[test]
	fn list_v2_pub_crate_reexport_alias_only_in_private_listing() {
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
mod inner {
    pub struct S;
}

pub(crate) use inner::S;
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let public_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), false, None).expect("list_v2 public");
		assert!(!public_items.iter().any(|i| i.path == "mini_crate::S"));

		let private_items = list_v2(&rs, dir.path().to_str().expect("path utf8"), true, None).expect("list_v2 private");
		assert!(private_items.iter().any(|i| i.path == "mini_crate::S"));
	}

	#[test]
	fn list_v2_target_path_suffix_filters_scope() {
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
pub mod foo {
    pub fn a() {}
}
pub mod bar {
    pub fn b() {}
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true);
		let target = format!("{}::foo", dir.path().display());
		let items = list_v2(&rs, &target, false, None).expect("list_v2");

		assert!(items.iter().any(|i| i.kind == SearchItemKind::Crate && i.path == "mini_crate"));
		assert!(items.iter().any(|i| i.path == "mini_crate::foo"));
		assert!(items.iter().any(|i| i.path == "mini_crate::foo::a"));
		assert!(!items.iter().any(|i| i.path == "mini_crate::bar"));
		assert!(!items.iter().any(|i| i.path == "mini_crate::bar::b"));
	}

	#[test]
	fn render_v2_markdown_smoke() {
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
pub mod foo {
    pub fn a() {}
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true).with_render_format(crate::RenderFormat::Markdown);
		let out = render_v2(&rs, dir.path().to_str().expect("path utf8"), false).expect("render_v2");

		assert!(out.contains("```rust"));
		assert!(out.contains("pub mod mini_crate"));
		assert!(out.contains("pub mod foo"));
	}

	#[test]
	fn search_v2_filters_and_renders() {
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
pub mod foo {
    pub fn a() {}
}

pub mod bar {
    pub fn b() {}
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true).with_render_format(crate::RenderFormat::Rust);
		let mut opts = SearchOptions::new("foo::a");
		opts.domains = SearchDomain::PATHS;

		let resp = search_v2(&rs, dir.path().to_str().expect("path utf8"), &opts).expect("search_v2");
		assert!(resp.rendered.contains("foo"));
		assert!(resp.rendered.contains("a"));
		assert!(!resp.rendered.contains("bar"));
	}

	#[test]
	fn render_v2_tuple_struct_keeps_tuple_fields() {
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
pub struct Tuple(pub u8);
pub struct Unit;
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true).with_render_format(crate::RenderFormat::Rust);
		let out = render_v2(&rs, dir.path().to_str().expect("path utf8"), false).expect("render_v2");

		assert!(out.contains("pub struct Tuple(pub u8);"));
		assert!(out.contains("pub struct Unit;"));
		assert!(!out.contains("pub struct Tuple;"));
	}

	#[test]
	fn search_v2_expand_containers_preserves_module_signature() {
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
pub(crate) mod hidden {
    pub mod nested {
        pub fn target() {}
    }
}
"#,
		)
		.expect("write lib.rs");

		let rs = Ripdoc::new().with_offline(true).with_render_format(crate::RenderFormat::Rust);
		let mut opts = SearchOptions::new("target");
		opts.domains = SearchDomain::NAMES;
		opts.include_private = true;
		opts.expand_containers = true;

		let resp = search_v2(&rs, dir.path().to_str().expect("path utf8"), &opts).expect("search_v2");

		assert!(resp.rendered.contains("pub(crate) mod hidden"));
		assert!(resp.rendered.contains("pub mod nested"));
		assert!(resp.rendered.contains("pub fn target()"));
	}
}
