use std::collections::HashSet;

use rust_format::{Config, Formatter, RustFmt};
use rustdoc_types::{Crate, Id};

use crate::error::Result;
use crate::markdown;

/// Supported high-level output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
	/// Render the crate as formatted Rust code (default).
	Rust,
	/// Render the crate using a Markdown-friendly layout.
	Markdown,
}

/// Selection of item identifiers used when rendering subsets of a crate.
#[derive(Debug, Clone, Default)]
pub struct RenderSelection {
	/// Item identifiers that directly satisfied the search query.
	matches: HashSet<Id>,
	/// Ancestor identifiers retained to preserve module hierarchy in output.
	context: HashSet<Id>,
	/// Matched containers whose children should be fully expanded.
	expanded: HashSet<Id>,
}

impl RenderSelection {
	/// Create a selection from explicit match and context sets.
	pub fn new(matches: HashSet<Id>, mut context: HashSet<Id>, expanded: HashSet<Id>) -> Self {
		for id in &matches {
			context.insert(*id);
		}
		Self {
			matches,
			context,
			expanded,
		}
	}

	/// Identifiers for items that should be fully rendered.
	pub fn matches(&self) -> &HashSet<Id> {
		&self.matches
	}

	/// Identifiers for items that should be kept to preserve hierarchy context.
	pub fn context(&self) -> &HashSet<Id> {
		&self.context
	}

	/// Containers that should expand to include all of their children.
	pub fn expanded(&self) -> &HashSet<Id> {
		&self.expanded
	}
}

/// Configurable renderer that turns rustdoc data into skeleton Rust source.
pub struct Renderer {
	/// Formatter used to produce tidy Rust output.
	pub formatter: RustFmt,
	/// Target output format.
	pub format: RenderFormat,
	/// Whether auto trait implementations should be included in the output.
	pub render_auto_impls: bool,
	/// Whether private items should be rendered.
	pub render_private_items: bool,
	/// Filter path relative to the crate root.
	pub filter: String,
	/// Optional selection restricting which items are rendered.
	pub selection: Option<RenderSelection>,
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}

impl Renderer {
	/// Create a renderer with default configuration.
	pub fn new() -> Self {
		let config = Config::new_str().option("brace_style", "PreferSameLine");
		Self {
			formatter: RustFmt::from_config(config),
			format: RenderFormat::Rust,
			render_auto_impls: false,
			render_private_items: false,
			filter: String::new(),
			selection: None,
		}
	}

	/// Apply a filter to output. The filter is a path BELOW the outermost module.
	pub fn with_filter(mut self, filter: &str) -> Self {
		self.filter = filter.to_string();
		self
	}

	/// Select the output format to render.
	pub fn with_format(mut self, format: RenderFormat) -> Self {
		self.format = format;
		self
	}

	/// Render auto-implemented traits like `Send` and `Sync`.
	pub fn with_auto_impls(mut self, render_auto_impls: bool) -> Self {
		self.render_auto_impls = render_auto_impls;
		self
	}

	/// Render private items?
	pub fn with_private_items(mut self, render_private_items: bool) -> Self {
		self.render_private_items = render_private_items;
		self
	}

	/// Restrict rendering to the provided selection.
	pub fn with_selection(mut self, selection: RenderSelection) -> Self {
		self.selection = Some(selection);
		self
	}

	/// Render a crate into formatted Rust source text.
	pub fn render(&self, crate_data: &Crate) -> Result<String> {
		use super::state::RenderState;

		let mut state = RenderState::new(self, crate_data);
		let raw_output = state.render()?;
		match self.format {
			RenderFormat::Rust => self.render_rust(&raw_output),
			RenderFormat::Markdown => self.render_markdown(raw_output),
		}
	}

	fn render_rust(&self, raw_output: &str) -> Result<String> {
		Ok(self.formatter.format_str(raw_output)?)
	}

	fn render_markdown(&self, raw_output: String) -> Result<String> {
		let formatted = self.render_rust(&raw_output)?;
		Ok(markdown::render_markdown(&formatted))
	}
}
