//! Test utilities shared across test modules.

use rustdoc_types::{Abi, FunctionHeader, Generics};

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
