use std::collections::HashSet;

use rust_format::{Config, Formatter, RustFmt};
use rustdoc_types::{Crate, Id};

use super::error::Result;
use crate::render::markdown;
use crate::render::utils::dedup_gap_markers;

/// Configuration for a render pass, specifying which items to include and how to format them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
	/// Format as valid Rust source code.
	Rust,
	/// Format as Markdown documentation.
	Markdown,
}

/// Selection of items to be rendered from a crate.
#[derive(Debug, Clone, Default)]
pub struct RenderSelection {
	/// Items that were explicitly matched by a query.
	matches: HashSet<Id>,
	/// Items that should be included for context (ancestors).
	context: HashSet<Id>,
	/// Items that should be fully expanded (all children rendered).
	expanded: HashSet<Id>,
	/// Items that should be rendered with their full source code.
	full_source: HashSet<Id>,
}

impl RenderSelection {
	/// Create a new render selection.
	pub fn new(
		matches: HashSet<Id>,
		mut context: HashSet<Id>,
		expanded: HashSet<Id>,
		full_source: HashSet<Id>,
	) -> Self {
		for id in &matches {
			context.insert(*id);
		}
		for id in &full_source {
			context.insert(*id);
		}
		Self {
			matches,
			context,
			expanded,
			full_source,
		}
	}

	/// Create a selection that includes everything in the crate.
	pub fn all() -> Self {
		Self::default()
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

	/// Identifiers for items that should be rendered with their full source code.
	pub fn full_source(&self) -> &HashSet<Id> {
		&self.full_source
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
	/// Whether to inject source filename labels in the output.
	pub render_source_labels: bool,
	/// Filter path relative to the crate root.
	pub filter: String,
	/// Optional selection restricting which items are rendered.
	pub selection: Option<RenderSelection>,
	/// Optional root path for resolving relative source files.
	pub source_root: Option<std::path::PathBuf>,
	/// Whether to use plain output (skip module nesting).
	pub plain: bool,
	/// Optional initial source file to suppress redundant headers.
	pub initial_current_file: Option<std::path::PathBuf>,
	/// Optional persistent visited set to avoid redundant item rendering across calls.
	pub visited: Option<std::sync::Arc<std::sync::Mutex<HashSet<Id>>>>,
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}

impl Renderer {
	/// Create a new renderer with default configuration.
	pub fn new() -> Self {
		let config = Config::new_str()
			.option("brace_style", "PreferSameLine")
			.option("hard_tabs", "true")
			.option("edition", "2021");
		Self {
			formatter: RustFmt::from_config(config),
			format: RenderFormat::Markdown,
			render_auto_impls: false,
			render_private_items: false,
			render_source_labels: true,
			filter: String::new(),
			selection: None,
			source_root: None,
			plain: false,
			initial_current_file: None,
			visited: None,
		}
	}

	/// Toggle plain output mode (skips module nesting).
	pub fn with_plain(mut self, plain: bool) -> Self {
		self.plain = plain;
		self
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

	/// Inject source filename labels?
	pub fn with_source_labels(mut self, render_source_labels: bool) -> Self {
		self.render_source_labels = render_source_labels;
		self
	}

	/// Restrict rendering to the provided selection.
	pub fn with_selection(mut self, selection: RenderSelection) -> Self {
		self.selection = Some(selection);
		self
	}

	/// Set the source root for resolving relative paths.
	pub fn with_source_root(mut self, root: std::path::PathBuf) -> Self {
		self.source_root = Some(root);
		self
	}

	/// Set the initial current file to suppress redundant headers.
	pub fn with_current_file(mut self, file: Option<std::path::PathBuf>) -> Self {
		self.initial_current_file = file;
		self
	}

	/// Set a persistent visited set.
	pub fn with_visited(mut self, visited: std::sync::Arc<std::sync::Mutex<HashSet<Id>>>) -> Self {
		self.visited = Some(visited);
		self
	}

	/// Render a crate into formatted Rust source text.
	pub fn render(&self, crate_data: &Crate) -> Result<String> {
		Ok(self.render_ext(crate_data)?.0)
	}

	/// Render a crate into formatted Rust source text, returning both output and final current file.
	pub fn render_ext(&self, crate_data: &Crate) -> Result<(String, Option<std::path::PathBuf>)> {
		use super::state::RenderState;

		let mut state = RenderState::new(self, crate_data);
		let raw_output = state.render()?;
		let final_file = state.current_file.clone();
		let output = match self.format {
			RenderFormat::Rust => self.render_rust(&raw_output)?,
			RenderFormat::Markdown => self.render_markdown(raw_output)?,
		};
		Ok((output, final_file))
	}

	fn render_rust(&self, raw_output: &str) -> Result<String> {
		match self.formatter.format_str(raw_output) {
			Ok(formatted) => Ok(self.apply_postprocessors(formatted)),
			Err(e) => {
				// Formatting failures are expected when rendering partial snippets.
				// Only emit a warning if explicitly requested.
				if std::env::var_os("RIPDOC_RUSTFMT_WARN").is_some() {
					eprintln!("Warning: An error occurred while formatting the source code: {e}");
				}
				Ok(self.apply_postprocessors(raw_output.to_string()))
			}
		}
	}

	fn render_markdown(&self, raw_output: String) -> Result<String> {
		let formatted = self.render_rust(&raw_output)?;
		Ok(markdown::render_markdown(&formatted))
	}

	fn apply_postprocessors(&self, rendered: String) -> String {
		dedup_gap_markers(&rendered)
	}
}
