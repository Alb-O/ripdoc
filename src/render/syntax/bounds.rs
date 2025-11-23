use rustdoc_types::{GenericBound, PolyTrait, TraitBoundModifier};

use super::path::render_path;

/// Render a generic bound expression into Rust syntax.
pub fn render_generic_bound(bound: &GenericBound) -> String {
	match bound {
		GenericBound::Use(_) => {
			// Omit unstable precise-capturing bounds to keep output valid
			String::new()
		}
		GenericBound::TraitBound {
			trait_,
			generic_params,
			modifier,
		} => {
			let modifier = match modifier {
				TraitBoundModifier::None => "",
				TraitBoundModifier::Maybe => "?",
				TraitBoundModifier::MaybeConst => "~const",
			};
			let poly_trait = PolyTrait {
				trait_: trait_.clone(),
				generic_params: generic_params.clone(),
			};
			match modifier {
				"" => render_path_as_poly_trait(&poly_trait),
				"~const" => format!("{modifier} {}", render_path_as_poly_trait(&poly_trait)),
				_ => format!("{modifier}{}", render_path_as_poly_trait(&poly_trait)),
			}
		}
		GenericBound::Outlives(lifetime) => lifetime.clone(),
	}
}

/// Render a `PolyTrait` including any generic parameters.
pub fn render_poly_trait(poly_trait: &PolyTrait) -> String {
	render_path_as_poly_trait(poly_trait)
}

/// Helper function to render a PolyTrait.
fn render_path_as_poly_trait(poly_trait: &PolyTrait) -> String {
	use super::generics::render_generic_param_def;

	let generic_params = if poly_trait.generic_params.is_empty() {
		String::new()
	} else {
		let params = poly_trait
			.generic_params
			.iter()
			.filter_map(render_generic_param_def)
			.collect::<Vec<_>>();

		if params.is_empty() {
			String::new()
		} else {
			format!("for<{}> ", params.join(", "))
		}
	};

	format!("{generic_params}{}", render_path(&poly_trait.trait_))
}

/// Render a comma-separated list of generic bounds.
pub fn render_generic_bounds(bounds: &[GenericBound]) -> String {
	let parts: Vec<String> = bounds
		.iter()
		.map(render_generic_bound)
		.filter(|s| !s.trim().is_empty())
		.collect();
	parts.join(" + ")
}

#[cfg(test)]
mod tests {
	use rustdoc_types::{Id, Path, TraitBoundModifier};

	use super::*;

	#[test]
	fn test_render_generic_bound_with_const_modifier() {
		// Test ~const modifier with a simple trait
		let trait_path = Path {
			id: Id(0),
			path: "MyTrait".to_string(),
			args: None,
		};
		let bound = GenericBound::TraitBound {
			trait_: trait_path,
			generic_params: vec![],
			modifier: TraitBoundModifier::MaybeConst,
		};

		let result = render_generic_bound(&bound);
		assert_eq!(result, "~const MyTrait");
	}

	#[test]
	fn test_render_generic_bound_with_const_modifier_and_path() {
		// Test ~const modifier with a trait path
		let trait_path = Path {
			id: Id(0),
			path: "fallback::DisjointBitOr".to_string(),
			args: None,
		};
		let bound = GenericBound::TraitBound {
			trait_: trait_path,
			generic_params: vec![],
			modifier: TraitBoundModifier::MaybeConst,
		};

		let result = render_generic_bound(&bound);
		assert_eq!(result, "~const fallback::DisjointBitOr");
	}

	#[test]
	fn test_render_generic_bound_with_maybe_modifier() {
		// Test ? modifier
		let trait_path = Path {
			id: Id(0),
			path: "Sized".to_string(),
			args: None,
		};
		let bound = GenericBound::TraitBound {
			trait_: trait_path,
			generic_params: vec![],
			modifier: TraitBoundModifier::Maybe,
		};

		let result = render_generic_bound(&bound);
		assert_eq!(result, "?Sized");
	}

	#[test]
	fn test_render_generic_bound_no_modifier() {
		// Test no modifier
		let trait_path = Path {
			id: Id(0),
			path: "Debug".to_string(),
			args: None,
		};
		let bound = GenericBound::TraitBound {
			trait_: trait_path,
			generic_params: vec![],
			modifier: TraitBoundModifier::None,
		};

		let result = render_generic_bound(&bound);
		assert_eq!(result, "Debug");
	}

	#[test]
	fn test_render_generic_bounds_omits_precise_capturing() {
		use rustdoc_types::{Id, Path, PreciseCapturingArg};

		// Prepare a normal trait bound
		let sized_path = Path {
			id: Id(0),
			path: "Sized".to_string(),
			args: None,
		};
		let trait_bound = GenericBound::TraitBound {
			trait_: sized_path,
			generic_params: vec![],
			modifier: TraitBoundModifier::None,
		};

		// And a precise-capturing `use<'a, T>` bound
		let use_bound = GenericBound::Use(vec![
			PreciseCapturingArg::Lifetime("'a".to_string()),
			PreciseCapturingArg::Param("T".to_string()),
		]);

		// When combined, only the valid trait bound should render
		let rendered = render_generic_bounds(&[trait_bound, use_bound]);
		assert_eq!(rendered, "Sized");
	}

	#[test]
	fn test_render_generic_bounds_only_precise_capturing() {
		use rustdoc_types::PreciseCapturingArg;

		let use_only = GenericBound::Use(vec![
			PreciseCapturingArg::Lifetime("'a".to_string()),
			PreciseCapturingArg::Param("T".to_string()),
		]);

		// If only `use<...>` is present, nothing should render
		let rendered = render_generic_bounds(&[use_only]);
		assert_eq!(rendered, "");
	}
}
