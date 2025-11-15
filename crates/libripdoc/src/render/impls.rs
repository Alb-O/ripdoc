use rustdoc_types::{Impl, Item, ItemEnum, Type, Visibility};

use super::state::RenderState;
use super::utils::ppush;
use crate::crateutils::*;

/// Traits that we render via `#[derive(...)]` annotations instead of explicit impl blocks.
pub const DERIVE_TRAITS: &[&str] = &[
	"Clone",
	"Copy",
	"Debug",
	"Default",
	"Display",
	"Eq",
	"Error",
	"FromStr",
	"Hash",
	"Ord",
	"PartialEq",
	"PartialOrd",
	"Send",
	"StructuralPartialEq",
	"Sync",
	// These are not built-in but are "well known" enough to treat specially
	"Serialize",
	"Deserialize",
];

/// Determine whether an impl block should be rendered in the output.
pub fn should_render_impl(impl_: &Impl, render_auto_impls: bool) -> bool {
	if impl_.is_synthetic && !render_auto_impls {
		return false;
	}

	if DERIVE_TRAITS.contains(&impl_.trait_.as_ref().map_or("", |t| t.path.as_str())) {
		return false;
	}

	if impl_.blanket_impl.is_some() {
		return false;
	}

	true
}

/// Render an implementation block, respecting filtering rules.
pub fn render_impl(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let mut output = docs(item);
	let impl_ = extract_item!(item, ItemEnum::Impl);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let selection_active = state.selection().is_some();
	let parent_expanded = match &impl_.for_ {
		Type::ResolvedPath(path) => state.selection_expands(&path.id),
		_ => false,
	};
	let expand_children = !selection_active || state.selection_expands(&item.id) || parent_expanded;

	if let Some(trait_) = &impl_.trait_
		&& let Some(trait_item) = state.crate_data.index.get(&trait_.id)
		&& !is_visible(state, trait_item)
	{
		return String::new();
	}

	let where_clause = render_where_clause(&impl_.generics);

	let trait_part = if let Some(trait_) = &impl_.trait_ {
		let trait_path = render_path(trait_);
		if !trait_path.is_empty() {
			format!("{trait_path} for ")
		} else {
			String::new()
		}
	} else {
		String::new()
	};

	output.push_str(&format!(
		"{}impl{} {}{}",
		if impl_.is_unsafe { "unsafe " } else { "" },
		render_generics(&impl_.generics),
		trait_part,
		render_type(&impl_.for_)
	));

	if !where_clause.is_empty() {
		output.push_str(&format!("\n{where_clause}"));
	}

	output.push_str(" {\n");

	let path_prefix = ppush(path_prefix, &render_type(&impl_.for_));
	let mut has_content = false;
	for item_id in &impl_.items {
		if let Some(item) = state.crate_data.index.get(item_id) {
			let is_trait_impl = impl_.trait_.is_some();
			if (!selection_active || expand_children || state.selection_context_contains(item_id))
				&& (is_trait_impl || is_visible(state, item))
			{
				let rendered = render_impl_item(state, &path_prefix, item, expand_children);
				if !rendered.is_empty() {
					output.push_str(&rendered);
					has_content = true;
				}
			}
		}
	}

	if !has_content {
		return String::new();
	}

	output.push_str("}\n\n");

	output
}

/// Render the item inside an impl block.
pub fn render_impl_item(
	state: &mut RenderState,
	path_prefix: &str,
	item: &Item,
	include_all: bool,
) -> String {
	if !include_all && !state.selection_context_contains(&item.id) {
		return String::new();
	}

	if state.should_filter(path_prefix, item) {
		return String::new();
	}

	match &item.inner {
		ItemEnum::Function(_) => render_function(state, item, false),
		ItemEnum::Constant { .. } => render_constant(state, item),
		ItemEnum::AssocType { .. } => render_associated_type(item),
		ItemEnum::TypeAlias(_) => render_type_alias(state, item),
		_ => String::new(),
	}
}

/// Render a trait definition.
pub fn render_trait(state: &RenderState, item: &Item) -> String {
	let mut output = docs(item);

	let trait_ = extract_item!(item, ItemEnum::Trait);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let selection = super::items::SelectionView::new(state, &item.id, true);

	let generics = render_generics(&trait_.generics);
	let where_clause = render_where_clause(&trait_.generics);

	let bounds = if !trait_.bounds.is_empty() {
		format!(": {}", render_generic_bounds(&trait_.bounds))
	} else {
		String::new()
	};

	let unsafe_prefix = if trait_.is_unsafe { "unsafe " } else { "" };

	output.push_str(&format!(
		"{}{}trait {}{}{}{} {{\n",
		render_vis(item),
		unsafe_prefix,
		render_name(item),
		generics,
		bounds,
		where_clause
	));

	for item_id in &trait_.items {
		if selection.includes_child(state, item_id) {
			let item = super::utils::must_get(state.crate_data, item_id);
			output.push_str(&render_trait_item(state, item, &selection));
		}
	}

	output.push_str("}\n\n");

	output
}

/// Render an item contained within a trait (method, associated type, etc.).
fn render_trait_item(
	state: &RenderState,
	item: &Item,
	selection: &super::items::SelectionView,
) -> String {
	if !selection.includes_child(state, &item.id) {
		return String::new();
	}
	match &item.inner {
		ItemEnum::Function(_) => render_function(state, item, true),
		ItemEnum::AssocConst { type_, value } => {
			let default_str = value
				.as_ref()
				.map(|d| format!(" = {d}"))
				.unwrap_or_default();
			format!(
				"const {}: {}{};\n",
				render_name(item),
				render_type(type_),
				default_str
			)
		}
		ItemEnum::AssocType {
			bounds,
			generics,
			type_,
		} => {
			let bounds_str = if !bounds.is_empty() {
				format!(": {}", render_generic_bounds(bounds))
			} else {
				String::new()
			};
			let generics_str = render_generics(generics);
			let default_str = type_
				.as_ref()
				.map(|d| format!(" = {}", render_type(d)))
				.unwrap_or_default();
			format!(
				"type {}{}{}{};\n",
				render_name(item),
				generics_str,
				bounds_str,
				default_str
			)
		}
		_ => String::new(),
	}
}

/// Determine whether an item should be rendered based on visibility settings.
fn is_visible(state: &RenderState, item: &Item) -> bool {
	state.config.render_private_items || matches!(item.visibility, Visibility::Public)
}

/// Render a function or method signature.
fn render_function(_state: &RenderState, item: &Item, is_trait_method: bool) -> String {
	let mut output = docs(item);
	let function = extract_item!(item, ItemEnum::Function);

	// Handle const, async, and unsafe keywords in the correct order
	let mut prefixes = Vec::new();
	if function.header.is_const {
		prefixes.push("const");
	}
	if function.header.is_async {
		prefixes.push("async");
	}
	if function.header.is_unsafe {
		prefixes.push("unsafe");
	}

	output.push_str(&format!(
		"{} {} fn {}{}({}){}{}",
		render_vis(item),
		prefixes.join(" "),
		render_name(item),
		render_generics(&function.generics),
		render_function_args(&function.sig),
		render_return_type(&function.sig),
		render_where_clause(&function.generics)
	));

	// Use semicolon for trait method declarations, empty body for implementations
	if is_trait_method && !function.has_body {
		output.push_str(";\n\n");
	} else {
		output.push_str(" {}\n\n");
	}

	output
}

/// Render a constant definition.
fn render_constant(_state: &RenderState, item: &Item) -> String {
	let mut output = docs(item);

	let (type_, const_) = extract_item!(item, ItemEnum::Constant { type_, const_ });
	output.push_str(&format!(
		"{}const {}: {} = {};\n\n",
		render_vis(item),
		render_name(item),
		render_type(type_),
		const_.expr
	));

	output
}

/// Render a type alias with generics, bounds, and visibility.
fn render_type_alias(_state: &RenderState, item: &Item) -> String {
	let type_alias = extract_item!(item, ItemEnum::TypeAlias);
	let mut output = docs(item);

	output.push_str(&format!(
		"{}type {}{}{}",
		render_vis(item),
		render_name(item),
		render_generics(&type_alias.generics),
		render_where_clause(&type_alias.generics),
	));

	output.push_str(&format!("= {};\n\n", render_type(&type_alias.type_)));

	output
}
