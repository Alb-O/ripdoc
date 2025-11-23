//! Core library for ripdoc, providing the main API for rendering Rust documentation.
//!
//! This crate provides the high-level `Ripdoc` API which orchestrates target resolution,
//! crate documentation generation, and rendering. It is designed to be UI-agnostic and
//! can be used by any frontend (CLI, GUI, language server, etc.).

/// Error helpers for the core API.
pub mod error;
/// Hierarchical tree structure for organizing list output.
pub mod list_tree;
/// Search and indexing utilities.
pub mod search;
use ripdoc_cargo::resolve_target;
/// Target parsing helpers exposed through ripdoc-cargo.
pub use ripdoc_cargo::target;
pub use ripdoc_render::{RenderFormat, Renderer};
use rustdoc_types::Crate;

pub use crate::error::Result;
pub use crate::list_tree::{CompactListTreeNode, ListTreeNode, build_list_tree};
pub use crate::search::{
	ListItem, SearchDomain, SearchItemKind, SearchOptions, SearchResponse, SourceLocation,
};
use crate::search::{SearchIndex, build_render_selection};

/// Ripdoc generates a skeletonized version of a Rust crate in a single page.
/// It produces syntactically valid Rust code with all implementations omitted.
///
/// The tool performs a 'cargo fetch' to ensure all referenced code is available locally,
/// then uses 'cargo doc' with the nightly toolchain to generate JSON output. This JSON
/// is parsed and used to render the skeletonized code. Users must have the nightly
/// Rust toolchain installed and available.
#[derive(Debug, Clone)]
pub struct Ripdoc {
	/// In offline mode Ripdoc will not attempt to fetch dependencies from the network.
	offline: bool,

	/// Whether to render auto-implemented traits.
	auto_impls: bool,

	/// Output format to use when rendering crates.
	render_format: RenderFormat,

	/// Whether to suppress output during processing.
	silent: bool,

	/// Cache configuration for rustdoc JSON output.
	cache_config: ripdoc_cargo::CacheConfig,
}

/// Check if the rendered output is essentially empty (just an empty module declaration).
/// This is used to detect binary-only crates with no public API.
fn is_empty_output(rendered: &str) -> bool {
	// Remove all whitespace: "pub mod name {}" becomes "pubmodname{}"
	let normalized: String = rendered.chars().filter(|c| !c.is_whitespace()).collect();

	// Match pattern: pubmod<identifier>{}
	normalized.starts_with("pubmod")
		&& normalized.ends_with("{}")
		&& normalized.matches('{').count() == 1
}

impl Default for Ripdoc {
	fn default() -> Self {
		Self::new()
	}
}

impl Ripdoc {
	/// Creates a new Ripdoc instance with default configuration.
	///
	/// # Target Format
	///
	/// A target specification is an entrypoint, followed by an optional path, with components
	/// separated by '::'.
	///
	///   entrypoint::path
	///
	/// An entrypoint can be:
	///
	/// - A path to a Rust file
	/// - A directory containing a Cargo.toml file
	/// - A module name
	/// - A package name. In this case the name can also include a version number, separated by an
	///   '@' symbol.
	///
	/// The path is a fully qualified path within the entrypoint.
	///
	/// # Examples of valid targets:
	///
	/// - src/lib.rs
	/// - my_module
	/// - serde
	/// - rustdoc-types
	/// - serde::Deserialize
	/// - serde@1.0
	/// - rustdoc-types::Crate
	/// - rustdoc_types::Crate
	pub fn new() -> Self {
		Self {
			offline: false,
			auto_impls: false,
			silent: false,
			render_format: RenderFormat::Markdown,
			cache_config: ripdoc_cargo::CacheConfig::default(),
		}
	}

	/// Enables or disables offline mode, which prevents Ripdoc from fetching dependencies from the
	/// network.
	pub fn with_offline(mut self, offline: bool) -> Self {
		self.offline = offline;
		self
	}

	/// Enables or disables rendering of auto-implemented traits.
	pub fn with_auto_impls(mut self, auto_impls: bool) -> Self {
		self.auto_impls = auto_impls;
		self
	}

	/// Selects the output format used when rendering crate documentation.
	pub fn with_render_format(mut self, format: RenderFormat) -> Self {
		self.render_format = format;
		self
	}

	/// Enables or disables silent mode, which suppresses output during processing.
	pub fn with_silent(mut self, silent: bool) -> Self {
		self.silent = silent;
		self
	}

	/// Enables or disables caching of rustdoc JSON output.
	pub fn with_cache(mut self, enabled: bool) -> Self {
		self.cache_config.enabled = enabled;
		self
	}

	/// Sets a custom cache directory for storing rustdoc JSON output.
	pub fn with_cache_dir(mut self, dir: std::path::PathBuf) -> Self {
		self.cache_config = self.cache_config.with_cache_dir(dir);
		self
	}

	/// Returns the parsed representation of the crate's API.
	///
	/// # Arguments
	/// * `target` - The target specification (see new() documentation for format)
	/// * `no_default_features` - Whether to build without default features
	/// * `all_features` - Whether to build with all features
	/// * `features` - List of specific features to enable
	/// * `private_items` - Whether to include private items in the output
	pub fn inspect(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
	) -> Result<Crate> {
		let rt = resolve_target(target, self.offline)?;
		Ok(rt.read_crate(
			no_default_features,
			all_features,
			features,
			private_items,
			self.silent,
			&self.cache_config,
		)?)
	}

	/// Execute a search against the crate and return the matched items along with a rendered skeleton.
	///
	/// The search respects the same target resolution logic as [`Self::render`], but only the
	/// matched items and their ancestors are emitted in the final skeleton.
	pub fn search(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		options: &SearchOptions,
	) -> Result<SearchResponse> {
		let rt = resolve_target(target, self.offline)?;
		let crate_data = rt.read_crate(
			no_default_features,
			all_features,
			features,
			options.include_private,
			self.silent,
			&self.cache_config,
		)?;

		let index = SearchIndex::build(
			&crate_data,
			options.include_private,
			Some(rt.package_root()),
		);
		let results = index.search(options);

		if results.is_empty() {
			return Ok(SearchResponse {
				results,
				rendered: String::new(),
			});
		}

		let selection = build_render_selection(&index, &results, options.expand_containers);
		let renderer = Renderer::default()
			.with_filter(&rt.filter)
			.with_auto_impls(self.auto_impls)
			.with_private_items(options.include_private)
			.with_format(self.render_format)
			.with_selection(selection);
		let rendered = renderer.render(&crate_data)?;

		Ok(SearchResponse { results, rendered })
	}

	/// Produce a lightweight listing of crate items, optionally filtered by a search query.
	pub fn list(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		include_private: bool,
		search: Option<&SearchOptions>,
	) -> Result<Vec<ListItem>> {
		let include_private = include_private
			|| search
				.map(|options| options.include_private)
				.unwrap_or(false);

		let rt = resolve_target(target, self.offline)?;
		let crate_data = rt.read_crate(
			no_default_features,
			all_features,
			features,
			include_private,
			self.silent,
			&self.cache_config,
		)?;

		let index = SearchIndex::build(&crate_data, include_private, Some(rt.package_root()));

		let mut results: Vec<ListItem> = if let Some(options) = search {
			index
				.search(options)
				.into_iter()
				.map(|result| ListItem {
					kind: result.kind,
					path: result.path_string,
					source: result.source,
				})
				.collect()
		} else {
			index
				.entries()
				.iter()
				.cloned()
				.map(|entry| ListItem {
					kind: entry.kind,
					path: entry.path_string,
					source: entry.source,
				})
				.collect()
		};

		results.retain(|item| item.kind != SearchItemKind::Use);

		Ok(results)
	}

	/// Render the crate target into a Rust skeleton without filtering.
	pub fn render(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
	) -> Result<String> {
		let rt = resolve_target(target, self.offline)?;
		let crate_data = rt.read_crate(
			no_default_features,
			all_features,
			features.clone(),
			private_items,
			self.silent,
			&self.cache_config,
		)?;

		let renderer = Renderer::default()
			.with_filter(&rt.filter)
			.with_auto_impls(self.auto_impls)
			.with_private_items(private_items)
			.with_format(self.render_format);

		let rendered = renderer.render(&crate_data)?;

		// If the public API is essentially empty and we weren't already including private items,
		// automatically retry with private items enabled (useful for binary-only crates)
		if !private_items && is_empty_output(&rendered) {
			let crate_data_private = rt.read_crate(
				no_default_features,
				all_features,
				features,
				true,
				self.silent,
				&self.cache_config,
			)?;

			let renderer_private = Renderer::default()
				.with_filter(&rt.filter)
				.with_auto_impls(self.auto_impls)
				.with_private_items(true)
				.with_format(RenderFormat::Rust);

			return Ok(renderer_private.render(&crate_data_private)?);
		}

		Ok(rendered)
	}

	/// Returns a pretty-printed version of the crate's JSON representation.
	///
	/// # Arguments
	/// * `target` - The target specification (see new() documentation for format)
	/// * `no_default_features` - Whether to build without default features
	/// * `all_features` - Whether to build with all features
	/// * `features` - List of specific features to enable
	/// * `private_items` - Whether to include private items in the JSON output
	pub fn raw_json(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
	) -> Result<String> {
		Ok(serde_json::to_string_pretty(&self.inspect(
			target,
			no_default_features,
			all_features,
			features,
			private_items,
		)?)?)
	}
}
