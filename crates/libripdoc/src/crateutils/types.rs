use rustdoc_types::Type;

use super::bounds::render_generic_bounds;
use super::path::render_path;

/// Render a type, tracking whether it is nested for parentheses handling.
pub fn render_type_inner(ty: &Type, nested: bool) -> String {
	match ty {
		Type::ResolvedPath(path) => {
			let args = path
				.args
				.as_ref()
				.map(|args| super::generics::render_generic_args(args))
				.unwrap_or_default();
			format!("{}{}", path.path.replace("$crate::", ""), args)
		}
		Type::DynTrait(dyn_trait) => {
			let traits = dyn_trait
				.traits
				.iter()
				.map(super::bounds::render_poly_trait)
				.collect::<Vec<_>>()
				.join(" + ");
			let lifetime = dyn_trait
				.lifetime
				.as_ref()
				.map(|lt| format!(" + {lt}"))
				.unwrap_or_default();

			let inner = format!("dyn {traits}{lifetime}");
			if nested
				&& (dyn_trait.lifetime.is_some()
					|| dyn_trait.traits.len() > 1
					|| traits.contains(" + "))
			{
				format!("({inner})")
			} else {
				inner
			}
		}
		Type::Generic(s) => s.clone(),
		Type::Primitive(s) => s.clone(),
		Type::FunctionPointer(f) => render_function_pointer(f),
		Type::Tuple(types) => {
			let inner = types
				.iter()
				.map(|ty| render_type_inner(ty, true))
				.collect::<Vec<_>>()
				.join(", ");
			format!("({inner})")
		}
		Type::Slice(ty) => format!("[{}]", render_type_inner(ty, true)),
		Type::Array { type_, len } => {
			format!("[{}; {len}]", render_type_inner(type_, true))
		}
		Type::ImplTrait(bounds) => {
			let bounds_str = render_generic_bounds(bounds);
			// If we're nested (e.g., inside a reference or function parameter) and have multiple bounds
			// (indicated by presence of '+' in the bounds string), we need parentheses to avoid ambiguity
			if nested && bounds_str.contains(" + ") {
				format!("(impl {bounds_str})")
			} else {
				format!("impl {bounds_str}")
			}
		}
		Type::Infer => "_".to_string(),
		Type::RawPointer { is_mutable, type_ } => {
			let mutability = if *is_mutable { "mut" } else { "const" };
			format!("*{mutability} {}", render_type_inner(type_, true))
		}
		Type::BorrowedRef {
			lifetime,
			is_mutable,
			type_,
		} => {
			let lifetime = lifetime
				.as_ref()
				.map(|lt| format!("{lt} "))
				.unwrap_or_default();
			let mutability = if *is_mutable { "mut " } else { "" };
			format!("&{lifetime}{mutability}{}", render_type_inner(type_, true))
		}
		Type::QualifiedPath {
			name,
			args,
			self_type,
			trait_,
		} => {
			let self_type_str = render_type_inner(self_type, true);
			let args_str = args
				.as_ref()
				.map(|a| super::generics::render_generic_args(a))
				.unwrap_or_default();

			if let Some(trait_) = trait_ {
				let trait_path = render_path(trait_);
				if !trait_path.is_empty() {
					format!("<{self_type_str} as {trait_path}>::{name}{args_str}")
				} else {
					format!("{self_type_str}::{name}{args_str}")
				}
			} else {
				format!("{self_type_str}::{name}{args_str}")
			}
		}
		Type::Pat { .. } => "/* pattern */".to_string(),
	}
}

/// Render a type without considering nesting.
pub fn render_type(ty: &Type) -> String {
	render_type_inner(ty, false)
}

/// Render a function pointer signature.
fn render_function_pointer(f: &rustdoc_types::FunctionPointer) -> String {
	let args = super::function::render_function_args(&f.sig);
	format!(
		"fn({}) {}",
		args,
		super::function::render_return_type(&f.sig)
	)
}
