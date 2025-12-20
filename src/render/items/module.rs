use rustdoc_types::{Item, ItemEnum};
use super::super::state::{GapController, RenderState};
use super::super::syntax::*;
use super::super::utils::ppush;
use super::render_item;

/// Render a module and its children.
pub fn render_module(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		if let Ok(source) =
			crate::render::utils::extract_source(span, state.config.source_root.as_deref())
		{
			return format!("{source}\n\n");
		}
	}

	let path_prefix = ppush(path_prefix, &render_name(item));

	let is_plain = state.config.plain;
	let mut output = if is_plain {
		String::new()
	} else {
		let mut head = format!("{}mod {} {{\n", render_vis(item), render_name(item));
		// Add module doc comment if present
		if state.should_module_doc(&path_prefix, item) && let Some(docs) = &item.docs {
			for line in docs.lines() {
				head.push_str(&format!("    //! {line}\n"));
			}
			head.push('\n');
		}
		head
	};

	let module = extract_item!(item, ItemEnum::Module);
	let gaps = GapController::new(if is_plain { "" } else { "    " });
	gaps.begin_section(state);

	for item_id in &module.items {
		if !state.selection_allows_child(&item.id, item_id) {
			state.mark_skipped();
			continue;
		}

		if let Some(inner_item) = state.crate_data.index.get(item_id) {
			let rendered = render_item(state, &path_prefix, inner_item, false);
			if !rendered.is_empty() {
				gaps.emit_if_needed(state, &mut output, &rendered);
				output.push_str(&rendered);
			} else {
				state.mark_skipped();
			}
		} else {
			state.mark_skipped();
		}
	}

	if !is_plain {
		output.push_str("}\n\n");
	}

	output
}
