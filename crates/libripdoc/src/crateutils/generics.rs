use rustdoc_types::{GenericArgs, GenericParamDef, GenericParamDefKind, Generics, WherePredicate};

use super::bounds::render_generic_bounds;
use super::types::render_type;

/// Render the generic parameter list for an item.
pub fn render_generics(generics: &Generics) -> String {
	let params: Vec<String> = generics
		.params
		.iter()
		.filter_map(render_generic_param_def)
		.collect();

	if params.is_empty() {
		String::new()
	} else {
		format!("<{}>", params.join(", "))
	}
}

/// Render an individual generic parameter definition.
pub fn render_generic_param_def(param: &GenericParamDef) -> Option<String> {
	match &param.kind {
		GenericParamDefKind::Lifetime { outlives } => {
			let outlives = if outlives.is_empty() {
				String::new()
			} else {
				format!(": {}", outlives.join(" + "))
			};
			Some(format!("{}{outlives}", param.name))
		}
		GenericParamDefKind::Type {
			bounds,
			default,
			is_synthetic,
		} => {
			if *is_synthetic {
				None
			} else {
				let bounds = if bounds.is_empty() {
					String::new()
				} else {
					let b = render_generic_bounds(bounds);
					if b.is_empty() {
						String::new()
					} else {
						format!(": {b}")
					}
				};
				let default = default
					.as_ref()
					.map(|ty| format!(" = {}", render_type(ty)))
					.unwrap_or_default();
				Some(format!("{}{bounds}{default}", param.name))
			}
		}
		GenericParamDefKind::Const { type_, default } => {
			let default = default
				.as_ref()
				.map(|expr| format!(" = {expr}"))
				.unwrap_or_default();
			Some(format!(
				"const {}: {}{default}",
				param.name,
				render_type(type_)
			))
		}
	}
}

/// Render concrete generic arguments used in a path.
pub fn render_generic_args(args: &GenericArgs) -> String {
	match args {
		GenericArgs::AngleBracketed { args, constraints } => {
			if args.is_empty() && constraints.is_empty() {
				String::new()
			} else {
				let args = args
					.iter()
					.map(render_generic_arg)
					.collect::<Vec<_>>()
					.join(", ");
				let bindings = constraints
					.iter()
					.map(render_type_constraint)
					.collect::<Vec<_>>()
					.join(", ");
				let all = if args.is_empty() {
					bindings
				} else if bindings.is_empty() {
					args
				} else {
					format!("{args}, {bindings}")
				};
				format!("<{all}>")
			}
		}
		GenericArgs::Parenthesized { inputs, output } => {
			let inputs = inputs
				.iter()
				.map(render_type)
				.collect::<Vec<_>>()
				.join(", ");
			let output = output
				.as_ref()
				.map(|ty| format!(" -> {}", render_type(ty)))
				.unwrap_or_default();
			format!("({inputs}){output}")
		}
		GenericArgs::ReturnTypeNotation => String::new(),
	}
}

/// Render an individual generic argument such as a lifetime or type.
fn render_generic_arg(arg: &rustdoc_types::GenericArg) -> String {
	use rustdoc_types::GenericArg;

	match arg {
		GenericArg::Lifetime(lt) => lt.clone(),
		GenericArg::Type(ty) => render_type(ty),
		GenericArg::Const(c) => {
			// Check if the expression contains macro variables ($ signs)
			// These come from unexpanded macros and would create invalid syntax
			if c.expr.contains('$') {
				"/* macro expression */".to_string()
			} else {
				c.expr.clone()
			}
		}
		GenericArg::Infer => "_".to_string(),
	}
}

/// Render a `where` clause for a generics block.
pub fn render_where_clause(generics: &Generics) -> String {
	let predicates: Vec<String> = generics
		.where_predicates
		.iter()
		.filter_map(render_where_predicate)
		.collect();
	if predicates.is_empty() {
		String::new()
	} else {
		format!(" where {}", predicates.join(", "))
	}
}

/// Render a single predicate within a `where` clause.
pub fn render_where_predicate(pred: &WherePredicate) -> Option<String> {
	use rustdoc_types::Type;

	match pred {
		WherePredicate::BoundPredicate {
			type_,
			bounds,
			generic_params,
		} => {
			// Check if this is a synthetic type
			if let Type::Generic(_name) = type_
				&& generic_params.iter().any(
					|param| matches!(&param.kind, GenericParamDefKind::Type { is_synthetic, .. } if *is_synthetic),
				) {
				return None;
			}

			let hrtb = if !generic_params.is_empty() {
				let params = generic_params
					.iter()
					.filter_map(render_generic_param_def)
					.collect::<Vec<_>>()
					.join(", ");
				if params.is_empty() {
					String::new()
				} else {
					format!("for<{params}> ")
				}
			} else {
				String::new()
			};

			let bounds_str = render_generic_bounds(bounds);
			if bounds_str.is_empty() {
				None
			} else {
				Some(format!("{hrtb}{}: {bounds_str}", render_type(type_)))
			}
		}
		WherePredicate::LifetimePredicate { lifetime, outlives } => {
			if outlives.is_empty() {
				Some(lifetime.clone())
			} else {
				Some(format!("{lifetime}: {}", outlives.join(" + ")))
			}
		}
		WherePredicate::EqPredicate { lhs, rhs } => {
			Some(format!("{} = {}", render_type(lhs), render_term(rhs)))
		}
	}
}

/// Render an associated type constraint with equality or bound semantics.
fn render_type_constraint(constraint: &rustdoc_types::AssocItemConstraint) -> String {
	use rustdoc_types::AssocItemConstraintKind;

	let binding_kind = match &constraint.binding {
		AssocItemConstraintKind::Equality(term) => format!(" = {}", render_term(term)),
		AssocItemConstraintKind::Constraint(bounds) => {
			let b = render_generic_bounds(bounds);
			if b.is_empty() {
				String::new()
			} else {
				format!(": {b}")
			}
		}
	};
	format!("{}{binding_kind}", constraint.name)
}

/// Render a `Term` appearing in associated type constraints.
fn render_term(term: &rustdoc_types::Term) -> String {
	use rustdoc_types::Term;

	match term {
		Term::Type(ty) => render_type(ty),
		Term::Constant(c) => c.expr.clone(),
	}
}
