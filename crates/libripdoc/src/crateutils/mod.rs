pub use self::bounds::{render_generic_bound, render_generic_bounds, render_poly_trait};
pub use self::function::{render_function_args, render_return_type};
pub use self::generics::{
	render_generic_args, render_generic_param_def, render_generics, render_where_clause,
};
pub use self::item::{docs, extract_item, render_associated_type, render_name, render_vis};
pub use self::path::render_path;
pub use self::types::{render_type, render_type_inner};

/// Generic parameter and bounds rendering utilities.
pub mod bounds;
/// Function signature rendering utilities.
pub mod function;
/// Generic argument and where clause rendering.
pub mod generics;
/// Item utilities including documentation and name rendering.
pub mod item;
/// Path and trait rendering utilities.
pub mod path;
/// Type rendering including primitives, compound types, and qualified paths.
pub mod types;
