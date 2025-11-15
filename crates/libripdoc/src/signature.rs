//! Signature rendering utilities for Rust items.
//!
//! This module provides functions to render compact, declaration-only signatures
//! for various Rust items (functions, structs, enums, traits, etc.). These signatures
//! are used both for search result display and as building blocks for full code rendering.

use rustdoc_types::{Item, ItemEnum, Variant};

use crate::crateutils::{
	extract_item, render_function_args, render_generic_bounds, render_generics, render_name,
	render_return_type, render_type, render_vis, render_where_clause,
};

/// Render a function signature (without body or docs).
pub fn function_signature(item: &Item) -> String {
	let function = extract_item!(item, ItemEnum::Function);

	let mut parts = Vec::new();
	let vis = render_vis(item);
	if !vis.trim().is_empty() {
		parts.push(vis.trim().to_string());
	}

	let mut qualifiers = Vec::new();
	if function.header.is_const {
		qualifiers.push("const");
	}
	if function.header.is_async {
		qualifiers.push("async");
	}
	if function.header.is_unsafe {
		qualifiers.push("unsafe");
	}
	if !qualifiers.is_empty() {
		parts.push(qualifiers.join(" "));
	}
	parts.push("fn".to_string());

	let mut signature = parts.join(" ");
	if !signature.is_empty() {
		signature.push(' ');
	}
	signature.push_str(&render_name(item));
	signature.push_str(&render_generics(&function.generics));
	signature.push('(');
	signature.push_str(&render_function_args(&function.sig));
	signature.push(')');
	signature.push_str(&render_return_type(&function.sig));
	signature.push_str(&render_where_clause(&function.generics));
	signature
}

/// Render a struct signature (without body or docs).
pub fn struct_signature(item: &Item) -> String {
	let struct_ = extract_item!(item, ItemEnum::Struct);
	format!(
		"{}struct {}{}{}",
		render_vis(item),
		render_name(item),
		render_generics(&struct_.generics),
		render_where_clause(&struct_.generics)
	)
	.trim()
	.to_string()
}

/// Render a union signature (without body or docs).
pub fn union_signature(item: &Item) -> String {
	let union_ = extract_item!(item, ItemEnum::Union);
	format!(
		"{}union {}{}{}",
		render_vis(item),
		render_name(item),
		render_generics(&union_.generics),
		render_where_clause(&union_.generics)
	)
	.trim()
	.to_string()
}

/// Render an enum signature (without variants or docs).
pub fn enum_signature(item: &Item) -> String {
	let enum_ = extract_item!(item, ItemEnum::Enum);
	format!(
		"{}enum {}{}{}",
		render_vis(item),
		render_name(item),
		render_generics(&enum_.generics),
		render_where_clause(&enum_.generics)
	)
	.trim()
	.to_string()
}

/// Render a trait signature (without methods or docs).
pub fn trait_signature(item: &Item) -> String {
	let trait_ = extract_item!(item, ItemEnum::Trait);
	let mut signature = String::new();
	signature.push_str(&render_vis(item));
	if trait_.is_unsafe {
		signature.push_str("unsafe ");
	}
	signature.push_str("trait ");
	signature.push_str(&render_name(item));
	signature.push_str(&render_generics(&trait_.generics));
	if !trait_.bounds.is_empty() {
		let bounds = render_generic_bounds(&trait_.bounds);
		if !bounds.is_empty() {
			signature.push_str(": ");
			signature.push_str(&bounds);
		}
	}
	signature.push_str(&render_where_clause(&trait_.generics));
	signature.trim().to_string()
}

/// Render a trait alias signature.
pub fn trait_alias_signature(item: &Item) -> String {
	let alias = extract_item!(item, ItemEnum::TraitAlias);
	let mut signature = String::new();
	signature.push_str(&render_vis(item));
	signature.push_str("trait ");
	signature.push_str(&render_name(item));
	signature.push_str(&render_generics(&alias.generics));
	let bounds = render_generic_bounds(&alias.params);
	if !bounds.is_empty() {
		signature.push_str(" = ");
		signature.push_str(&bounds);
	}
	signature.push_str(&render_where_clause(&alias.generics));
	signature.trim().to_string()
}

/// Render a type alias signature.
pub fn type_alias_signature(item: &Item) -> String {
	let type_alias = extract_item!(item, ItemEnum::TypeAlias);
	format!(
		"{}type {}{}{} = {}",
		render_vis(item),
		render_name(item),
		render_generics(&type_alias.generics),
		render_where_clause(&type_alias.generics),
		render_type(&type_alias.type_)
	)
	.trim()
	.to_string()
}

/// Render a constant signature.
pub fn constant_signature(item: &Item) -> String {
	let (type_, _const_) = extract_item!(item, ItemEnum::Constant { type_, const_ });
	format!(
		"{}const {}: {}",
		render_vis(item),
		render_name(item),
		render_type(type_)
	)
	.trim()
	.to_string()
}

/// Render a static signature.
pub fn static_signature(item: &Item) -> String {
	let static_ = extract_item!(item, ItemEnum::Static);
	format!(
		"{}static {}: {}",
		render_vis(item),
		render_name(item),
		render_type(&static_.type_)
	)
	.trim()
	.to_string()
}

/// Render an associated constant signature.
pub fn assoc_const_signature(item: &Item) -> String {
	let (type_, _value) = extract_item!(item, ItemEnum::AssocConst { type_, value });
	format!("const {}: {}", render_name(item), render_type(type_))
}

/// Render an associated type signature.
pub fn assoc_type_signature(item: &Item) -> String {
	let (_generics, bounds, type_) = extract_item!(
		item,
		ItemEnum::AssocType {
			generics,
			bounds,
			type_
		}
	);
	if let Some(ty) = type_ {
		format!("type {} = {}", render_name(item), render_type(ty))
	} else if !bounds.is_empty() {
		format!(
			"type {}: {}",
			render_name(item),
			render_generic_bounds(bounds)
		)
	} else {
		format!("type {}", render_name(item))
	}
}

/// Render a macro signature.
pub fn macro_signature(item: &Item) -> String {
	format!("macro {}", render_name(item))
}

/// Render a proc macro signature.
pub fn proc_macro_signature(item: &Item) -> String {
	let proc_macro = extract_item!(item, ItemEnum::ProcMacro);
	let prefix = match proc_macro.kind {
		rustdoc_types::MacroKind::Derive => "#[proc_macro_derive]",
		rustdoc_types::MacroKind::Attr => "#[proc_macro_attribute]",
		rustdoc_types::MacroKind::Bang => "#[proc_macro]",
	};
	format!("{} {}", prefix, render_name(item))
}

/// Render a use/import signature.
pub fn use_signature(item: &Item) -> String {
	let import = extract_item!(item, ItemEnum::Use);
	let mut signature = String::new();
	signature.push_str(&render_vis(item));
	signature.push_str("use ");
	signature.push_str(&import.source);
	if import.name != import.source.split("::").last().unwrap_or(&import.source) {
		signature.push_str(" as ");
		signature.push_str(&import.name);
	}
	if import.is_glob {
		signature.push_str("::*");
	}
	signature.trim().to_string()
}

/// Render a primitive type signature.
pub fn primitive_signature(item: &Item) -> String {
	format!("primitive {}", render_name(item))
}

/// Render a module signature.
pub fn module_signature(item: &Item) -> String {
	format!("{}mod {}", render_vis(item), render_name(item))
		.trim()
		.to_string()
}

/// Render a struct field signature.
pub fn field_signature(item: &Item) -> String {
	let ty = extract_item!(item, ItemEnum::StructField);
	let mut signature = String::new();
	let vis = render_vis(item);
	if !vis.trim().is_empty() {
		signature.push_str(vis.trim());
		signature.push(' ');
	}
	if let Some(name) = item.name.as_deref() {
		signature.push_str(name);
		signature.push_str(": ");
	}
	signature.push_str(&render_type(ty));
	signature
}

/// Render an enum variant signature (including fields if present).
pub fn variant_signature(
	item: &Item,
	variant: &Variant,
	field_lookup: impl Fn(&rustdoc_types::Id) -> Option<String>,
) -> String {
	let mut signature = render_name(item);
	match &variant.kind {
		rustdoc_types::VariantKind::Plain => {}
		rustdoc_types::VariantKind::Tuple(fields) => {
			let parts: Vec<String> = fields
				.iter()
				.filter_map(|field_id| field_id.as_ref().and_then(&field_lookup))
				.collect();
			signature.push('(');
			signature.push_str(&parts.join(", "));
			signature.push(')');
		}
		rustdoc_types::VariantKind::Struct { fields, .. } => {
			let parts: Vec<String> = fields.iter().filter_map(&field_lookup).collect();
			signature.push_str(" { ");
			signature.push_str(&parts.join(", "));
			signature.push_str(" }");
		}
	}
	signature
}
