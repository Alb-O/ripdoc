use rustdoc_types::{Item, ItemEnum, StructKind, VariantKind, Visibility};

use crate::crateutils::*;

use super::state::RenderState;
use super::utils::{ppush, must_get};
use super::impls::{render_impl, should_render_impl, DERIVE_TRAITS};
use super::macros::{render_macro, render_proc_macro};

/// Render an item into Rust source text.
pub fn render_item(state: &mut RenderState, path_prefix: &str, item: &Item, force_private: bool) -> String {
	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	if state.should_filter(path_prefix, item) {
		return String::new();
	}

	let output = match &item.inner {
		ItemEnum::Module(_) => render_module(state, path_prefix, item),
		ItemEnum::Struct(_) => render_struct(state, path_prefix, item),
		ItemEnum::Enum(_) => render_enum(state, path_prefix, item),
		ItemEnum::Trait(_) => super::impls::render_trait(state, item),
		ItemEnum::Use(_) => render_use(state, path_prefix, item),
		ItemEnum::Function(_) => render_function_item(state, item, false),
		ItemEnum::Constant { .. } => render_constant_item(state, item),
		ItemEnum::TypeAlias(_) => render_type_alias_item(state, item),
		ItemEnum::Macro(_) => render_macro(item),
		ItemEnum::ProcMacro(_) => render_proc_macro(item),
		_ => String::new(),
	};

	if !force_private && !is_visible(state, item) {
		String::new()
	} else {
		output
	}
}

/// Render a module and its children.
pub fn render_module(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let path_prefix = ppush(path_prefix, &render_name(item));
	let mut output = format!("{}mod {} {{\n", render_vis(item), render_name(item));
	// Add module doc comment if present
	if state.should_module_doc(&path_prefix, item)
		&& let Some(docs) = &item.docs
	{
		for line in docs.lines() {
			output.push_str(&format!("    //! {line}\n"));
		}
		output.push('\n');
	}

	let module = extract_item!(item, ItemEnum::Module);

	for item_id in &module.items {
		let item = must_get(state.crate_data, item_id);
		output.push_str(&render_item(state, &path_prefix, item, false));
	}

	output.push_str("}\n\n");
	output
}

/// Render a struct declaration and its fields.
pub fn render_struct(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let mut output = docs(item);

	let struct_ = extract_item!(item, ItemEnum::Struct);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let selection_active = state.selection().is_some();
	let expand_children = if selection_active {
		state.selection_expands(&item.id)
	} else {
		false
	};
	let force_fields = selection_active && expand_children;

	// Collect inline traits
	let mut inline_traits = Vec::new();
	for impl_id in &struct_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if impl_.is_synthetic {
			continue;
		}

		if let Some(trait_) = &impl_.trait_
			&& let Some(name) = trait_.path.split("::").last()
			&& DERIVE_TRAITS.contains(&name)
		{
			inline_traits.push(name);
		}
	}

	// Add derive attribute if we found any inline traits
	if !inline_traits.is_empty() {
		output.push_str(&format!("#[derive({})]\n", inline_traits.join(", ")));
	}

	let generics = render_generics(&struct_.generics);
	let where_clause = render_where_clause(&struct_.generics);

	match &struct_.kind {
		StructKind::Unit => {
			output.push_str(&format!(
				"{}struct {}{}{};\n\n",
				render_vis(item),
				render_name(item),
				generics,
				where_clause
			));
		}
		StructKind::Tuple(fields) => {
			let fields_str = fields
				.iter()
				.filter_map(|field| {
					field.as_ref().and_then(|id| {
						if !expand_children && !state.selection_context_contains(id) {
							return None;
						}
						let field_item = must_get(state.crate_data, id);
						let ty = extract_item!(field_item, ItemEnum::StructField);
						if !is_visible(state, field_item) {
							Some("_".to_string())
						} else {
							Some(format!("{}{}", render_vis(field_item), render_type(ty)))
						}
					})
				})
				.collect::<Vec<_>>()
				.join(", ");

			if expand_children || !fields_str.is_empty() {
				output.push_str(&format!(
					"{}struct {}{}({}){};\n\n",
					render_vis(item),
					render_name(item),
					generics,
					fields_str,
					where_clause
				));
			}
		}
		StructKind::Plain { fields, .. } => {
			output.push_str(&format!(
				"{}struct {}{}{} {{\n",
				render_vis(item),
				render_name(item),
				generics,
				where_clause
			));
			for field in fields {
				let rendered = render_struct_field(state, field, force_fields);
				if !rendered.is_empty() {
					output.push_str(&rendered);
				}
			}
			output.push_str("}\n\n");
		}
	}

	// Render impl blocks
	for impl_id in &struct_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if should_render_impl(impl_, state.config.render_auto_impls)
			&& state.selection_allows_child(&item.id, impl_id)
		{
			output.push_str(&render_impl(state, path_prefix, impl_item));
		}
	}

	output
}

/// Render a struct field, optionally forcing visibility.
pub fn render_struct_field(state: &RenderState, field_id: &rustdoc_types::Id, force: bool) -> String {
	let field_item = must_get(state.crate_data, field_id);

	if state.selection().is_some() && !force && !state.selection_context_contains(field_id) {
		return String::new();
	}

	if !(force || is_visible(state, field_item)) {
		return String::new();
	}

	let ty = extract_item!(field_item, ItemEnum::StructField);
	let mut out = String::new();
	out.push_str(&docs(field_item));
	out.push_str(&format!(
		"{}{}: {},\n",
		render_vis(field_item),
		render_name(field_item),
		render_type(ty)
	));
	out
}

/// Render an enum definition, including variants.
pub fn render_enum(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let mut output = docs(item);

	let enum_ = extract_item!(item, ItemEnum::Enum);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let selection_active = state.selection().is_some();
	let include_all_variants = state.selection_expands(&item.id);

	// Collect inline traits
	let mut inline_traits = Vec::new();
	for impl_id in &enum_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if impl_.is_synthetic {
			continue;
		}

		if let Some(trait_) = &impl_.trait_
			&& let Some(name) = trait_.path.split("::").last()
			&& DERIVE_TRAITS.contains(&name)
		{
			inline_traits.push(name);
		}
	}

	// Add derive attribute if we found any inline traits
	if !inline_traits.is_empty() {
		output.push_str(&format!("#[derive({})]\n", inline_traits.join(", ")));
	}

	let generics = render_generics(&enum_.generics);
	let where_clause = render_where_clause(&enum_.generics);

	output.push_str(&format!(
		"{}enum {}{}{} {{\n",
		render_vis(item),
		render_name(item),
		generics,
		where_clause
	));

	for variant_id in &enum_.variants {
		if !selection_active
			|| include_all_variants
			|| state.selection_context_contains(variant_id)
		{
			let variant_item = must_get(state.crate_data, variant_id);
			let include_variant_fields = include_all_variants
				|| !selection_active
				|| state.selection_matches(&variant_item.id);
			let rendered = render_enum_variant(state, variant_item, include_variant_fields);
			if !rendered.is_empty() {
				output.push_str(&rendered);
			}
		}
	}

	output.push_str("}\n\n");

	// Render impl blocks
	for impl_id in &enum_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if should_render_impl(impl_, state.config.render_auto_impls)
			&& state.selection_allows_child(&item.id, impl_id)
		{
			output.push_str(&render_impl(state, path_prefix, impl_item));
		}
	}

	output
}

/// Render a single enum variant.
pub fn render_enum_variant(state: &RenderState, item: &Item, include_all_fields: bool) -> String {
	let selection_active = state.selection().is_some();

	if selection_active && !include_all_fields && !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let mut output = docs(item);

	let variant = extract_item!(item, ItemEnum::Variant);

	output.push_str(&format!("    {}", render_name(item)));

	match &variant.kind {
		VariantKind::Plain => {}
		VariantKind::Tuple(fields) => {
			let fields_str = fields
				.iter()
				.filter_map(|field| {
					field.as_ref().and_then(|id| {
						if selection_active
							&& !include_all_fields && !state.selection_context_contains(id)
						{
							return None;
						}
						let field_item = must_get(state.crate_data, id);
						let ty = extract_item!(field_item, ItemEnum::StructField);
						Some(render_type(ty))
					})
				})
				.collect::<Vec<_>>()
				.join(", ");
			output.push_str(&format!("({fields_str})"));
		}
		VariantKind::Struct { fields, .. } => {
			output.push_str(" {\n");
			for field in fields {
				if !selection_active
					|| include_all_fields
					|| state.selection_context_contains(field)
				{
					let rendered = render_struct_field(state, field, include_all_fields || !selection_active);
					if !rendered.is_empty() {
						output.push_str(&rendered);
					}
				}
			}
			output.push_str("    }");
		}
	}

	if let Some(discriminant) = &variant.discriminant {
		output.push_str(&format!(" = {}", discriminant.expr));
	}

	output.push_str(",\n");

	output
}

/// Render a `use` statement, applying filter rules for private modules.
pub fn render_use(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	use super::utils::escape_path;

	let import = extract_item!(item, ItemEnum::Use);

	if import.is_glob {
		if let Some(source_id) = &import.id
			&& let Some(source_item) = state.crate_data.index.get(source_id)
		{
			let mut output = String::new();

			// Handle glob imports from modules (pub use module::*)
			if matches!(source_item.inner, ItemEnum::Module(_)) {
				let module = extract_item!(source_item, ItemEnum::Module);
				for item_id in &module.items {
					if let Some(item) = state.crate_data.index.get(item_id)
						&& is_visible(state, item)
					{
						output.push_str(&render_item(state, path_prefix, item, true));
					}
				}
				return output;
			}

			// Handle glob imports from enums (pub use Enum::*)
			if matches!(source_item.inner, ItemEnum::Enum(_)) {
				let enum_ = extract_item!(source_item, ItemEnum::Enum);
				for variant_id in &enum_.variants {
					if let Some(variant) = state.crate_data.index.get(variant_id)
						&& is_visible(state, variant)
					{
						output.push_str(&render_item(state, path_prefix, variant, true));
					}
				}
				return output;
			}

			// For other types, fall through to render as-is
		}
		// If we can't resolve the glob import, fall back to rendering it as-is
		return format!("pub use {}::*;\n", escape_path(&import.source));
	}

	if let Some(imported_item) = import
		.id
		.as_ref()
		.and_then(|id| state.crate_data.index.get(id))
	{
		return render_item(state, path_prefix, imported_item, true);
	}

	let mut output = docs(item);
	if import.name != import.source.split("::").last().unwrap_or(&import.source) {
		// Check if the alias itself needs escaping
		let escaped_name = if crate::keywords::is_reserved_word(import.name.as_str()) {
			format!("r#{}", import.name)
		} else {
			import.name.clone()
		};
		output.push_str(&format!(
			"pub use {} as {};\n",
			escape_path(&import.source),
			escaped_name
		));
	} else {
		output.push_str(&format!("pub use {};\n", escape_path(&import.source)));
	}

	output
}

/// Determine whether an item should be rendered based on visibility settings.
fn is_visible(state: &RenderState, item: &Item) -> bool {
	state.config.render_private_items || matches!(item.visibility, Visibility::Public)
}

/// Render a function or method signature.
fn render_function_item(_state: &RenderState, item: &Item, is_trait_method: bool) -> String {
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
fn render_constant_item(_state: &RenderState, item: &Item) -> String {
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
fn render_type_alias_item(_state: &RenderState, item: &Item) -> String {
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
