use rustdoc_types::{Crate, Id, Item};

/// Retrieve an item from the crate index, panicking if it is missing.
pub fn must_get<'a>(crate_data: &'a Crate, id: &Id) -> &'a Item {
	crate_data.index.get(id).unwrap()
}

/// Append `name` to a path prefix using `::` separators.
pub fn ppush(path_prefix: &str, name: &str) -> String {
	if path_prefix.is_empty() {
		name.to_string()
	} else {
		format!("{path_prefix}::{name}")
	}
}

/// Escape reserved keywords in a path by adding raw identifier prefixes when needed.
pub fn escape_path(path: &str) -> String {
	use crate::is_reserved_word;

	path.split("::")
		.map(|segment| {
			// Some keywords like 'crate', 'self', 'super' cannot be raw identifiers
			if segment == "crate" || segment == "self" || segment == "super" || segment == "Self" {
				segment.to_string()
			} else if is_reserved_word(segment) {
				format!("r#{}", segment)
			} else {
				segment.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("::")
}

/// Classification describing how a filter string matches a path.
#[derive(Debug, PartialEq)]
pub enum FilterMatch {
	/// The filter exactly matches the path.
	Hit,
	/// The filter matches a prefix of the path.
	Prefix,
	/// The filter matches a suffix of the path.
	Suffix,
	/// The filter does not match the path.
	Miss,
}
