use rustdoc_types::{Item, ItemEnum, Visibility};

/// Convenience macro to destructure `rustdoc_types::Item` variants during rendering.
#[macro_export]
macro_rules! extract_item {
    ($item:expr, $variant:path) => {
        match &$item.inner {
            $variant(inner) => inner,
            _ => panic!("Expected {}, found {:?}", stringify!($variant), $item.inner),
        }
    };
    ($item:expr, $variant:path { $($field:ident),+ }) => {
        match &$item.inner {
            $variant { $($field,)+ .. } => ($($field,)+),
            _ => panic!("Expected {}, found {:?}", stringify!($variant), $item.inner),
        }
    };
}

pub use extract_item;

/// Format documentation comments as triple-slash lines.
pub fn docs(item: &Item) -> String {
	let mut output = String::new();
	if let Some(docs) = &item.docs {
		for line in docs.lines() {
			output.push_str(&format!("/// {line}\n"));
		}
	}
	output
}

/// Render the visibility modifier for an item if it is public.
pub fn render_vis(item: &Item) -> String {
	match &item.visibility {
		Visibility::Public => "pub ".to_string(),
		_ => String::new(),
	}
}

/// Render an item name, escaping Rust keywords when necessary.
pub fn render_name(item: &Item) -> String {
	use crate::is_reserved_word;

	item.name.as_deref().map_or_else(
		|| "?".to_string(),
		|n| {
			if is_reserved_word(n) {
				format!("r#{n}")
			} else {
				n.to_string()
			}
		},
	)
}

/// Render an associated type definition, including defaults and bounds.
pub fn render_associated_type(item: &Item) -> String {
	use super::bounds::render_generic_bounds;
	use super::types::render_type;

	let (bounds, default) = extract_item!(item, ItemEnum::AssocType { bounds, type_ });

	let bounds_str = if !bounds.is_empty() {
		format!(": {}", render_generic_bounds(bounds))
	} else {
		String::new()
	};
	let default_str = default
		.as_ref()
		.map(|d| format!(" = {}", render_type(d)))
		.unwrap_or_default();
	format!("type {}{bounds_str}{default_str};\n", render_name(item))
}
