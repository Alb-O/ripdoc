/// Enum and variant rendering logic.
pub mod enums;
/// Module rendering logic.
pub mod module;
/// Rendering logic for functions, constants, and type aliases.
pub mod others;
/// Struct and field rendering logic.
pub mod structs;
/// Import and re-export rendering logic.
pub mod use_stmt;

pub use enums::render_enum;
pub use module::render_module;
pub use others::{render_constant_item, render_function_item, render_type_alias_item};
use rustdoc_types::{Id, Item, ItemEnum, Visibility};
pub use structs::render_struct;
pub use use_stmt::render_use;

use super::impls::DERIVE_TRAITS;
use super::macros::{render_macro, render_proc_macro};
use super::state::RenderState;
use super::utils::must_get;

pub(crate) fn extracted_source_looks_like_item(item: &Item, source: &str) -> bool {
	fn first_code_line(source: &str) -> Option<&str> {
		for line in source.lines() {
			let trimmed = line.trim_start();
			if trimmed.is_empty() {
				continue;
			}
			if trimmed.starts_with("//") || trimmed.starts_with("/*") {
				continue;
			}
			if trimmed.starts_with('#') {
				continue;
			}
			return Some(trimmed);
		}
		None
	}

	let Some(line) = first_code_line(source) else {
		return false;
	};

	match &item.inner {
		ItemEnum::Function(_) => line.contains("fn "),
		ItemEnum::Impl(_) => line.starts_with("impl ") || line.starts_with("unsafe impl "),
		ItemEnum::Struct(_) => line.contains("struct "),
		ItemEnum::Enum(_) => line.contains("enum "),
		ItemEnum::Trait(_) => line.contains("trait "),
		ItemEnum::TypeAlias(_) => line.contains("type "),
		ItemEnum::Constant { .. } => line.contains("const "),
		ItemEnum::Static(_) => line.contains("static "),
		ItemEnum::Use(_) => line.contains("use "),
		ItemEnum::Module(_) => line.contains("mod "),
		ItemEnum::Macro(_) => line.contains("macro_rules!") || line.contains("macro "),
		ItemEnum::ProcMacro(_) => true,
		_ => true,
	}
}

/// Captures how the current selection affects an item's children.
pub(crate) struct SelectionView {
	active: bool,
	expands_self: bool,
}

impl SelectionView {
	pub(crate) fn new(state: &RenderState, id: &Id, expands_when_inactive: bool) -> Self {
		let active = state.selection().is_some();
		let expands_self = if active {
			state.selection_expands(id)
		} else {
			expands_when_inactive
		};
		Self {
			active,
			expands_self,
		}
	}

	pub(crate) fn includes_child(&self, state: &RenderState, child_id: &Id) -> bool {
		if !self.active {
			return true;
		}
		self.expands_self || state.selection_context_contains(child_id)
	}

	pub(crate) fn force_children(&self) -> bool {
		self.active && self.expands_self
	}

	pub(crate) fn is_active(&self) -> bool {
		self.active
	}

	pub(crate) fn expands_self(&self) -> bool {
		self.expands_self
	}
}

/// Collect trait names rendered via `#[derive]` for the provided impl list.
pub(crate) fn collect_inline_traits<'a>(state: &'a RenderState, impls: &[Id]) -> Vec<&'a str> {
	let mut inline_traits = Vec::new();
	for impl_id in impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if impl_.is_synthetic {
			continue;
		}

		if let Some(trait_) = &impl_.trait_
			&& let Some(name) = trait_.path.split("::").last()
			&& DERIVE_TRAITS.contains(&name)
		{
			inline_traits.push(name);
		}
	}
	inline_traits
}

/// Render an item into Rust source text.
pub fn render_item(
	state: &mut RenderState,
	path_prefix: &str,
	item: &Item,
	force_private: bool,
) -> String {
	// Early visibility check to avoid rendering children of non-visible containers.
	// This prevents items from being marked as visited when they're rendered inside
	// a private module that will ultimately be discarded.
	if !force_private && !is_visible(state, item) {
		return String::new();
	}

	if !matches!(item.inner, ItemEnum::Module(_) | ItemEnum::Impl(_))
		&& state.visited.contains(&item.id)
	{
		return String::new();
	}

	if matches!(item.inner, ItemEnum::Module(_)) {
		let is_new = state.visited.insert(item.id);
		if !is_new {
			return String::new();
		}
	}

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	if state.should_filter(path_prefix, item) {
		return String::new();
	}

	if state.selection_is_full_source(&item.id)
		&& let Some(span) = &item.span
		&& let Ok(source) =
			crate::render::utils::extract_source(span, state.config.source_root.as_deref())
		&& extracted_source_looks_like_item(item, &source)
	{
		state.visited.insert(item.id);
		return format!("{source}\n\n");
	}

	let mut output = match &item.inner {
		ItemEnum::Module(_) => render_module(state, path_prefix, item),
		ItemEnum::Struct(_) => render_struct(state, path_prefix, item),
		ItemEnum::Enum(_) => render_enum(state, path_prefix, item),
		ItemEnum::Trait(_) => super::impls::render_trait(state, item),
		ItemEnum::Use(_) => render_use(state, path_prefix, item),
		ItemEnum::Function(_) => render_function_item(state, item, false),
		ItemEnum::Constant { .. } => render_constant_item(state, item),
		ItemEnum::TypeAlias(_) => render_type_alias_item(state, item),
		ItemEnum::Macro(_) => render_macro(state, item),
		ItemEnum::ProcMacro(_) => render_proc_macro(state, item),
		_ => String::new(),
	};

	if !output.is_empty() {
		state.visited.insert(item.id);
	}

	if !output.is_empty()
		&& state.config.render_source_labels
		&& !matches!(item.inner, ItemEnum::Use(_))
		&& !(matches!(item.inner, ItemEnum::Module(_)) && state.config.plain)
		&& let Some(span) = &item.span
		&& state.current_file.as_ref() != Some(&span.filename)
	{
		state.current_file = Some(span.filename.clone());
		let label = format!("// ripdoc:source: {}\n\n", span.filename.display());
		output = format!("{}{}", label, output);
	}

	output
}

/// Determine whether an item should be rendered based on visibility settings.
pub(crate) fn is_visible(state: &RenderState, item: &Item) -> bool {
	state.config.render_private_items || matches!(item.visibility, Visibility::Public)
}
