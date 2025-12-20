use rustdoc_types::{Id, Item, ItemEnum};
use super::super::state::{GapController, RenderState};
use super::super::syntax::*;
use super::super::utils::escape_path;
use super::{is_visible, render_item};

pub(crate) enum UseResolution {
	Items(Vec<Id>),
	Alias { source: String, alias: String },
	Simple(String),
}

/// Render a `use` statement, applying filter rules for private modules.
pub fn render_use(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let import = extract_item!(item, ItemEnum::Use);
	let resolution = resolve_use(state, import);

	match resolution {
		UseResolution::Items(items) => {
			let mut output = String::new();
			let gaps = GapController::new("");
			gaps.begin_section(state);
			let mut any_rendered = false;

			// Render all expanded items as a single group so we don't emit gap
			// markers between items that originated from the same `use`.
			for item_id in items {
				if let Some(item) = state.crate_data.index.get(&item_id) {
					let rendered = render_item(state, path_prefix, item, true);
					if !rendered.is_empty() {
						gaps.emit_if_needed(state, &mut output, &rendered);
						output.push_str(&rendered);
						any_rendered = true;
					} else if !any_rendered {
						// Track that we skipped something before the first render.
						state.mark_skipped();
					}
				}
			}
			// Prevent skipped items inside the expansion from queuing another
			// gap marker for the parent module.
			state.clear_pending_gap();

			output
		}
		UseResolution::Alias { source, alias } => {
			let mut output = docs(item);
			output.push_str(&format!("pub use {source} as {alias};\n"));
			output
		}
		UseResolution::Simple(source) => {
			let mut output = docs(item);
			output.push_str(&format!("pub use {source};\n"));
			output
		}
	}
}

fn resolve_use(state: &RenderState, import: &rustdoc_types::Use) -> UseResolution {
	if import.is_glob {
		return resolve_glob_use(state, import);
	}

	if let Some(imported_item) = import
		.id
		.as_ref()
		.and_then(|id| state.crate_data.index.get(id))
	{
		return UseResolution::Items(vec![imported_item.id]);
	}

	resolve_alias_use(import)
}

fn resolve_glob_use(state: &RenderState, import: &rustdoc_types::Use) -> UseResolution {
	let Some(source_id) = &import.id else {
		return UseResolution::Simple(format!("{}::*", escape_path(&import.source)));
	};
	let Some(source_item) = state.crate_data.index.get(source_id) else {
		return UseResolution::Simple(format!("{}::*", escape_path(&import.source)));
	};

	match &source_item.inner {
		ItemEnum::Module(module) => {
			let items = module
				.items
				.iter()
				.filter(|item_id| {
					state
						.crate_data
						.index
						.get(item_id)
						.map(|item| is_visible(state, item))
						.unwrap_or(false)
				})
				.cloned()
				.collect();
			UseResolution::Items(items)
		}
		ItemEnum::Enum(enum_) => {
			let items = enum_
				.variants
				.iter()
				.filter(|variant_id| {
					state
						.crate_data
						.index
						.get(variant_id)
						.map(|variant| is_visible(state, variant))
						.unwrap_or(false)
				})
				.cloned()
				.collect();
			UseResolution::Items(items)
		}
		_ => UseResolution::Simple(format!("{}::*", escape_path(&import.source))),
	}
}

fn resolve_alias_use(import: &rustdoc_types::Use) -> UseResolution {
	use super::super::syntax::is_reserved_word;

	let source = escape_path(&import.source);
	let last_segment = import.source.split("::").last().unwrap_or(&import.source);
	if import.name != last_segment {
		let escaped_name = if is_reserved_word(import.name.as_str()) {
			format!("r#{}", import.name)
		} else {
			import.name.clone()
		};
		UseResolution::Alias {
			source,
			alias: escaped_name,
		}
	} else {
		UseResolution::Simple(source)
	}
}
