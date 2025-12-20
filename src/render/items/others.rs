use rustdoc_types::{Item, ItemEnum};
use super::super::state::RenderState;
use super::super::syntax::*;
use super::extracted_source_looks_like_item;

/// Render a function or method signature.
pub fn render_function_item(state: &RenderState, item: &Item, is_trait_method: bool) -> String {
	if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		if let Ok(source) = crate::render::utils::extract_source(span, state.config.source_root.as_deref())
			&& extracted_source_looks_like_item(item, &source)
		{
			return format!("{source}\n\n");
		}
	}

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
pub fn render_constant_item(state: &RenderState, item: &Item) -> String {
	if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		if let Ok(source) = crate::render::utils::extract_source(span, state.config.source_root.as_deref())
			&& extracted_source_looks_like_item(item, &source)
		{
			return format!("{source}\n\n");
		}
	}

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
pub fn render_type_alias_item(state: &RenderState, item: &Item) -> String {
	if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		if let Ok(source) = crate::render::utils::extract_source(span, state.config.source_root.as_deref())
			&& extracted_source_looks_like_item(item, &source)
		{
			return format!("{source}\n\n");
		}
	}

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
