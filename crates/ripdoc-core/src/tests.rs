use std::collections::HashMap;

use rustdoc_types::{
	Abi, Crate, Function, FunctionHeader, FunctionSignature, Generics, Id, Impl, Item, ItemEnum,
	Module, Path, Struct, StructKind, Target, Trait, Type, Visibility,
};

use crate::search::*;

/// Create an empty Generics instance for testing.
pub fn empty_generics() -> Generics {
	Generics {
		params: Vec::new(),
		where_predicates: Vec::new(),
	}
}

/// Create a default FunctionHeader for testing.
pub fn default_header() -> FunctionHeader {
	FunctionHeader {
		is_const: false,
		is_unsafe: false,
		is_async: false,
		abi: Abi::Rust,
	}
}

fn fixture_crate() -> Crate {
	let root = Id(0);
	let widget = Id(1);
	let widget_field = Id(2);
	let widget_impl = Id(3);
	let render_method = Id(4);
	let helper_fn = Id(5);
	let paintable_trait = Id(6);
	let paint_method = Id(7);

	let mut index = HashMap::new();

	index.insert(
		root,
		Item {
			id: root,
			crate_id: 0,
			name: Some("fixture".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Fixture root module".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Module(Module {
				is_crate: true,
				items: vec![widget, helper_fn, paintable_trait, widget_impl],
				is_stripped: false,
			}),
		},
	);

	index.insert(
		widget,
		Item {
			id: widget,
			crate_id: 0,
			name: Some("Widget".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Widget docs highlight the component".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Struct(Struct {
				kind: StructKind::Plain {
					fields: vec![widget_field],
					has_stripped_fields: false,
				},
				generics: empty_generics(),
				impls: vec![widget_impl],
			}),
		},
	);

	index.insert(
		widget_field,
		Item {
			id: widget_field,
			crate_id: 0,
			name: Some("id".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Identifier for Widget".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::StructField(Type::Primitive("u32".into())),
		},
	);

	index.insert(
		widget_impl,
		Item {
			id: widget_impl,
			crate_id: 0,
			name: None,
			span: None,
			visibility: Visibility::Public,
			docs: None,
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Impl(Impl {
				is_unsafe: false,
				generics: empty_generics(),
				provided_trait_methods: Vec::new(),
				trait_: None,
				for_: Type::ResolvedPath(Path {
					path: "Widget".into(),
					id: widget,
					args: None,
				}),
				items: vec![render_method],
				is_negative: false,
				is_synthetic: false,
				blanket_impl: None,
			}),
		},
	);

	index.insert(
		render_method,
		Item {
			id: render_method,
			crate_id: 0,
			name: Some("render".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Render the widget".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Function(Function {
				sig: FunctionSignature {
					inputs: vec![(
						"self".into(),
						Type::BorrowedRef {
							lifetime: None,
							is_mutable: false,
							type_: Box::new(Type::Generic("Self".into())),
						},
					)],
					output: Some(Type::Primitive("u32".into())),
					is_c_variadic: false,
				},
				generics: empty_generics(),
				header: default_header(),
				has_body: true,
			}),
		},
	);

	index.insert(
		helper_fn,
		Item {
			id: helper_fn,
			crate_id: 0,
			name: Some("helper".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Helper docs mention Widget".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Function(Function {
				sig: FunctionSignature {
					inputs: vec![("count".into(), Type::Primitive("i32".into()))],
					output: Some(Type::ResolvedPath(Path {
						path: "Widget".into(),
						id: widget,
						args: None,
					})),
					is_c_variadic: false,
				},
				generics: empty_generics(),
				header: default_header(),
				has_body: true,
			}),
		},
	);

	index.insert(
		paintable_trait,
		Item {
			id: paintable_trait,
			crate_id: 0,
			name: Some("Paintable".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Paintable trait handles colors".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Trait(Trait {
				is_auto: false,
				is_unsafe: false,
				is_dyn_compatible: true,
				items: vec![paint_method],
				generics: empty_generics(),
				bounds: Vec::new(),
				implementations: Vec::new(),
			}),
		},
	);

	index.insert(
		paint_method,
		Item {
			id: paint_method,
			crate_id: 0,
			name: Some("paint".into()),
			span: None,
			visibility: Visibility::Public,
			docs: Some("Paint method docs".into()),
			links: HashMap::new(),
			attrs: Vec::new(),
			deprecation: None,
			inner: ItemEnum::Function(Function {
				sig: FunctionSignature {
					inputs: vec![(
						"self".into(),
						Type::BorrowedRef {
							lifetime: None,
							is_mutable: false,
							type_: Box::new(Type::Generic("Self".into())),
						},
					)],
					output: None,
					is_c_variadic: false,
				},
				generics: empty_generics(),
				header: default_header(),
				has_body: false,
			}),
		},
	);

	Crate {
		root,
		crate_version: Some("0.1.0".into()),
		includes_private: false,
		index,
		paths: HashMap::new(),
		external_crates: HashMap::new(),
		target: Target {
			triple: "test-target".into(),
			target_features: Vec::new(),
		},
		format_version: 0,
	}
}

fn build_index() -> SearchIndex {
	let crate_data = fixture_crate();
	SearchIndex::build(&crate_data, false, None)
}

#[test]
fn name_domain_matches_impl_method() {
	let index = build_index();
	let mut options = SearchOptions::new("render");
	options.domains = SearchDomain::NAMES;
	let results = index.search(&options);
	assert!(results.iter().any(|r| r.raw_name == "render"));
	assert!(
		results
			.iter()
			.all(|r| r.matched.contains(SearchDomain::NAMES))
	);
}

#[test]
fn multi_domain_hits_report_all_matches() {
	let index = build_index();
	let mut options = SearchOptions::new("Widget");
	options.domains = SearchDomain::NAMES | SearchDomain::DOCS;
	let results = index.search(&options);
	let widget = results
		.into_iter()
		.find(|r| r.raw_name == "Widget")
		.expect("Widget result");
	assert!(widget.matched.contains(SearchDomain::NAMES));
	assert!(widget.matched.contains(SearchDomain::DOCS));
}

#[test]
fn default_domains_exclude_paths() {
	let defaults = SearchDomain::default();
	assert!(defaults.contains(SearchDomain::NAMES));
	assert!(defaults.contains(SearchDomain::DOCS));
	assert!(defaults.contains(SearchDomain::SIGNATURES));
	assert!(!defaults.contains(SearchDomain::PATHS));
}

#[test]
fn path_domain_matches_impl_member() {
	let index = build_index();
	let mut options = SearchOptions::new("fixture::Widget::render");
	options.domains = SearchDomain::PATHS;
	let results = index.search(&options);
	assert!(results.iter().any(|r| r.raw_name == "render"));
}

#[test]
fn signature_domain_matches_free_function() {
	let index = build_index();
	let mut options = SearchOptions::new("fn helper");
	options.domains = SearchDomain::SIGNATURES;
	let results = index.search(&options);
	assert!(results.iter().any(|r| r.raw_name == "helper"));
}

#[test]
fn case_sensitive_toggle_affects_results() {
	let index = build_index();
	let mut options = SearchOptions::new("widget docs");
	options.domains = SearchDomain::DOCS;
	options.case_sensitive = true;
	assert!(index.search(&options).is_empty());
	options.case_sensitive = false;
	assert!(!index.search(&options).is_empty());
}

#[test]
fn negative_query_returns_empty() {
	let index = build_index();
	let options = SearchOptions::new("missing");
	assert!(index.search(&options).is_empty());
}

#[test]
fn describe_domains_lists_selected_flags() {
	assert_eq!(
		super::describe_domains(SearchDomain::empty()),
		Vec::<&str>::new()
	);
	assert_eq!(super::describe_domains(SearchDomain::NAMES), vec!["name"]);
	assert_eq!(
		super::describe_domains(SearchDomain::NAMES | SearchDomain::DOCS),
		vec!["name", "doc"]
	);
}
