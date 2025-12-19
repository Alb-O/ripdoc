use std::collections::HashSet;

use rustdoc_types::{Crate, Id, Item};

use super::core::{RenderSelection, Renderer};
use super::error::{Result, RipdocError};
use super::utils::{FilterMatch, GAP_MARKER, ends_with_gap, must_get, ppush, starts_with_gap};

/// Tracks whether a gap marker should be inserted before the next rendered item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapState {
	/// No pending gap marker.
	Clear,
	/// Items were skipped and a gap marker should be emitted before the next render.
	Pending,
}

/// Helper that centralizes gap marker insertion rules for renderers.
pub struct GapController<'a> {
	indent: &'a str,
}

impl<'a> GapController<'a> {
	/// Create a new controller using the provided indentation prefix.
	pub fn new(indent: &'a str) -> Self {
		Self { indent }
	}

	/// Initialize any section-level tracking before iterating children.
	pub fn begin_section(&self, _state: &mut RenderState<'_, '_>) {
		// Reserved for future section-level behavior; intentionally a no-op today.
	}

	/// Emit a gap marker if state indicates one is pending.
	pub fn emit_if_needed(
		&self,
		state: &mut RenderState<'_, '_>,
		output: &mut String,
		next_block: &str,
	) {
		state.emit_gap_if_needed(output, self.indent, next_block);
	}

	/// Build a child indentation prefix relative to the current controller.
	pub fn child_prefix_for(&self, indent: &str) -> String {
		format!("{}{}", self.indent, indent)
	}
}

/// Mutable rendering context shared across helper functions.
pub struct RenderState<'a, 'b> {
	/// Reference to the immutable renderer configuration.
	pub config: &'a Renderer,
	/// Crate metadata produced by rustdoc.
	pub crate_data: &'b Crate,
	/// Tracks whether any item matched the configured filter.
	pub filter_matched: bool,
	/// Tracks whether items were skipped (used for gap markers in search mode).
	pub gap_state: GapState,
	/// Tracks items already rendered to prevent infinite recursion/redundancy.
	pub visited: HashSet<Id>,
	/// Tracks the current source file being rendered to detect transitions.
	pub current_file: Option<std::path::PathBuf>,
}

impl<'a, 'b> RenderState<'a, 'b> {
	/// Create a new render state.
	pub fn new(config: &'a Renderer, crate_data: &'b Crate) -> Self {
		Self {
			config,
			crate_data,
			filter_matched: false,
			gap_state: GapState::Clear,
			visited: HashSet::new(),
			current_file: None,
		}
	}

	/// Render the crate, applying filters and formatting output.
	pub fn render(&mut self) -> Result<String> {
		use super::items::render_item;

		// The root item is always a module
		let output = render_item(
			self,
			"",
			must_get(self.crate_data, &self.crate_data.root),
			false,
		);

		if !self.config.filter.is_empty() && !self.filter_matched {
			return Err(RipdocError::FilterNotMatched(self.config.filter.clone()));
		}

		Ok(output)
	}

	/// Return the active render selection, if any.
	pub fn selection(&self) -> Option<&RenderSelection> {
		self.config.selection.as_ref()
	}

	/// Determine whether the selection context includes a particular item.
	pub fn selection_context_contains(&self, id: &Id) -> bool {
		match self.selection() {
			Some(selection) => selection.context().contains(id),
			None => true,
		}
	}

	/// Check if an item was an explicit match in the selection.
	pub fn selection_matches(&self, id: &Id) -> bool {
		match self.selection() {
			Some(selection) => selection.matches().contains(id),
			None => false,
		}
	}

	/// Determine whether a matched container should expand its children in the rendered output.
	pub fn selection_expands(&self, id: &Id) -> bool {
		match self.selection() {
			Some(selection) => selection.expanded().contains(id),
			None => true,
		}
	}

	/// Determine whether a child item should be rendered based on its parent and selection context.
	pub fn selection_allows_child(&self, parent_id: &Id, child_id: &Id) -> bool {
		if self.selection().is_none() {
			return true;
		}
		self.selection_expands(parent_id) || self.selection_context_contains(child_id)
	}

	/// Determine whether an item is filtered out by the configured path filter.
	pub fn should_filter(&mut self, path_prefix: &str, item: &Item) -> bool {
		// We never filter the root module - filters operate under the root.
		if item.id == self.crate_data.root {
			return false;
		}

		if self.config.filter.is_empty() {
			return false;
		}
		match self.filter_match(path_prefix, item) {
			FilterMatch::Hit => {
				self.filter_matched = true;
				false
			}
			FilterMatch::Prefix | FilterMatch::Suffix => false,
			FilterMatch::Miss => true,
		}
	}

	/// Evaluate how the current filter matches a candidate path.
	pub fn filter_match(&self, path_prefix: &str, item: &Item) -> FilterMatch {
		let item_path = if let Some(name) = &item.name {
			ppush(path_prefix, name)
		} else {
			return FilterMatch::Prefix;
		};

		let filter_components: Vec<&str> = self.config.filter.split("::").collect();
		let item_components: Vec<&str> = item_path.split("::").skip(1).collect();

		if filter_components == item_components {
			FilterMatch::Hit
		} else if filter_components.starts_with(&item_components) {
			FilterMatch::Prefix
		} else if item_components.starts_with(&filter_components) {
			FilterMatch::Suffix
		} else {
			FilterMatch::Miss
		}
	}

	/// Determine whether a module should emit a `//!` doc comment header.
	pub fn should_module_doc(&self, path_prefix: &str, item: &Item) -> bool {
		if self.config.filter.is_empty() {
			return true;
		}
		matches!(
			self.filter_match(path_prefix, item),
			FilterMatch::Hit | FilterMatch::Suffix
		)
	}

	/// Mark that an item was skipped (not rendered).
	pub fn mark_skipped(&mut self) {
		if self.selection().is_some() {
			self.gap_state = GapState::Pending;
		}
	}

	fn should_emit_gap(&self) -> bool {
		self.selection().is_some() && matches!(self.gap_state, GapState::Pending)
	}

	fn emit_gap_if_needed(&mut self, output: &mut String, indent: &str, next_block: &str) {
		if !self.should_emit_gap() {
			return;
		}

		if !ends_with_gap(output) && !starts_with_gap(next_block) {
			output.push_str(indent);
			output.push_str(GAP_MARKER);
			output.push('\n');
		}

		self.gap_state = GapState::Clear;
	}

	/// Reset any pending gap marker, used when inner renderers handle their own gaps.
	pub fn clear_pending_gap(&mut self) {
		self.gap_state = GapState::Clear;
	}
}
