use rustdoc_types::{FunctionSignature, Type};

use super::types::render_type;

/// Render a function's parameter list, including names and types.
pub fn render_function_args(decl: &FunctionSignature) -> String {
	decl.inputs
		.iter()
		.map(|(name, ty)| {
			if name == "self" {
				match ty {
					Type::BorrowedRef { is_mutable, .. } => {
						if *is_mutable {
							"&mut self".to_string()
						} else {
							"&self".to_string()
						}
					}
					Type::ResolvedPath(path) => {
						if path.path == "Self" && path.args.is_none() {
							"self".to_string()
						} else {
							format!("self: {}", render_type(ty))
						}
					}
					Type::Generic(name) => {
						if name == "Self" {
							"self".to_string()
						} else {
							format!("self: {}", render_type(ty))
						}
					}
					_ => format!("self: {}", render_type(ty)),
				}
			} else {
				format!("{name}: {}", render_type(ty))
			}
		})
		.collect::<Vec<_>>()
		.join(", ")
}

/// Render a function's return type, including the `->` separator when needed.
pub fn render_return_type(decl: &FunctionSignature) -> String {
	match &decl.output {
		Some(ty) => format!("-> {}", render_type(ty)),
		None => String::new(),
	}
}
