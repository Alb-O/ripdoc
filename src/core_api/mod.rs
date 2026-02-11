//! Core library for ripdoc, providing the main API for rendering Rust documentation.
//!
//! This crate provides the high-level `Ripdoc` API which orchestrates target resolution,
//! crate documentation generation, and rendering. It is designed to be UI-agnostic and
//! can be used by any frontend (CLI, GUI, language server, etc.).

/// Error helpers for the core API.
pub mod error;
/// Hierarchical tree structure for organizing list output.
pub mod list_tree;
/// Pattern utilities for search query handling.
pub mod pattern;
/// Search and indexing utilities.
pub mod search;
/// Backend selection and routing.
pub mod backend;
use std::collections::HashSet;
use std::fs;

use rustdoc_types::Crate;

pub use self::error::Result;
pub use self::list_tree::{ListTreeNode, build_list_tree};
pub use self::search::{
	ListItem, SearchDomain, SearchItemKind, SearchOptions, SearchResponse, SourceLocation,
};
use self::search::{SearchIndex, build_render_selection};
use super::cargo_utils::resolve_target;
/// Target parsing helpers exposed through cargo_utils.
pub use super::cargo_utils::target;
pub use super::render::{RenderFormat, Renderer};

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

	/// Whether to inject source filename labels.
	render_source_labels: bool,

	/// Whether to suppress output during processing.
	silent: bool,

	/// Cache configuration for rustdoc JSON output.
	cache_config: super::cargo_utils::CacheConfig,
}

/// Check if the rendered output is essentially empty (just an empty module declaration).
/// This is used to detect binary-only crates with no public API.
#[allow(dead_code)]
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
			render_source_labels: true,
			cache_config: super::cargo_utils::CacheConfig::default(),
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

	/// Enables or disables source filename labels.
	pub fn with_source_labels(mut self, enabled: bool) -> Self {
		self.render_source_labels = enabled;
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

	/// Returns the currently configured render format.
	pub fn render_format(&self) -> RenderFormat {
		self.render_format
	}

	/// Returns whether source filename labels are enabled.
	pub fn render_source_labels(&self) -> bool {
		self.render_source_labels
	}

	/// Returns whether ripdoc is running in offline mode.
	pub fn offline(&self) -> bool {
		self.offline
	}

	/// Returns whether ripdoc is running in silent mode.
	pub fn silent(&self) -> bool {
		self.silent
	}

	/// Returns the active cache configuration.
	pub fn cache_config(&self) -> &super::cargo_utils::CacheConfig {
		&self.cache_config
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
	) -> Result<Vec<Crate>> {
		let resolved_targets = resolve_target(target, self.offline)?;
		let mut crates = Vec::with_capacity(resolved_targets.len());
		for rt in resolved_targets {
			crates.push(rt.read_crate(
				no_default_features,
				all_features,
				features.clone(),
				private_items,
				self.silent,
				&self.cache_config,
			)?);
		}
		Ok(crates)
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
		implementation: bool,
		raw_source: bool,
	) -> Result<SearchResponse> {
		let resolved_targets = resolve_target(target, self.offline)?;
		let mut all_results = Vec::new();
		let mut all_rendered = Vec::new();

		for rt in resolved_targets {
			let crate_data = rt.read_crate(
				no_default_features,
				all_features,
				features.clone(),
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
				continue;
			}

			let mut full_source_ids = HashSet::new();
			if implementation {
				for res in &results {
					full_source_ids.insert(res.item_id);
				}
			}

			let mut raw_files_content = String::new();
			if raw_source {
				let mut seen_files = HashSet::new();
				for res in &results {
					if let Some(item) = crate_data.index.get(&res.item_id)
						&& let Some(span) = &item.span
						&& seen_files.insert(span.filename.clone())
					{
						let abs_path = if span.filename.is_absolute() {
							span.filename.clone()
						} else {
							rt.package_root().join(&span.filename)
						};
						if let Ok(content) = fs::read_to_string(&abs_path) {
							raw_files_content.push_str(&format!(
								"// ripdoc:source: {}\n\n{}\n\n",
								span.filename.display(),
								content
							));
						}
					}
				}
			}

			let selection = build_render_selection(
				&index,
				&results,
				options.expand_containers,
				full_source_ids,
			);
			let renderer = Renderer::default()
				.with_filter(&rt.filter)
				.with_auto_impls(self.auto_impls)
				.with_private_items(options.include_private)
				.with_source_labels(self.render_source_labels)
				.with_format(self.render_format)
				.with_source_root(rt.package_root().to_path_buf())
				.with_selection(selection);
			let mut rendered = renderer.render(&crate_data)?;

			if !raw_files_content.is_empty() {
				rendered = format!("{}\n---\n\n{}", raw_files_content, rendered);
			}

			all_results.extend(results);
			all_rendered.push(rendered);
		}

		Ok(SearchResponse {
			results: all_results,
			rendered: all_rendered.join("\n"),
		})
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

		#[cfg(feature = "v2-ts")]
		if backend::active_backend() == backend::BackendKind::TreeSitter {
			let _ = (no_default_features, all_features, &features);
			return crate::v2::list_v2(self, target, include_private, search);
		}

		let resolved_targets = resolve_target(target, self.offline)?;
		let mut all_results = Vec::new();

		for rt in resolved_targets {
			let crate_data = rt.read_crate(
				no_default_features,
				all_features,
				features.clone(),
				include_private,
				self.silent,
				&self.cache_config,
			)?;

			let index = SearchIndex::build(&crate_data, include_private, Some(rt.package_root()));

			let results: Vec<ListItem> = if let Some(options) = search {
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
			all_results.extend(results);
		}

		all_results.retain(|item| item.kind != SearchItemKind::Use);

		Ok(all_results)
	}

	/// Render the crate target into a Rust skeleton without filtering.
	pub fn render(
		&self,
		target: &str,
		no_default_features: bool,
		all_features: bool,
		features: Vec<String>,
		private_items: bool,
		implementation: bool,
		raw_source: bool,
	) -> Result<String> {
		let resolved_targets = resolve_target(target, self.offline)?;
		let mut rendered_outputs = Vec::new();

		for rt in resolved_targets {
			let crate_data = rt.read_crate(
				no_default_features,
				all_features,
				features.clone(),
				private_items,
				self.silent,
				&self.cache_config,
			)?;

			let mut full_source_ids = HashSet::new();
			let mut raw_files_content = String::new();

			if implementation || raw_source {
				let index = SearchIndex::build(&crate_data, private_items, Some(rt.package_root()));
				let mut options = SearchOptions::new(&rt.filter);
				options.include_private = private_items;
				options.domains = SearchDomain::PATHS;
				let results = index.search(&options);

				if implementation {
					for res in &results {
						full_source_ids.insert(res.item_id);
					}
				}

				if raw_source {
					let mut seen_files = HashSet::new();
					for res in &results {
						if let Some(item) = crate_data.index.get(&res.item_id)
							&& let Some(span) = &item.span
							&& seen_files.insert(span.filename.clone())
						{
							let abs_path = if span.filename.is_absolute() {
								span.filename.clone()
							} else {
								rt.package_root().join(&span.filename)
							};
							if let Ok(content) = fs::read_to_string(&abs_path) {
								raw_files_content.push_str(&format!(
									"// ripdoc:source: {}\n\n{}\n\n",
									span.filename.display(),
									content
								));
							}
						}
					}
				}
			}

			let mut renderer = Renderer::default()
				.with_filter(&rt.filter)
				.with_auto_impls(self.auto_impls)
				.with_private_items(private_items)
				.with_source_labels(self.render_source_labels)
				.with_format(self.render_format)
				.with_source_root(rt.package_root().to_path_buf());

			if !full_source_ids.is_empty() {
				let index = SearchIndex::build(&crate_data, private_items, Some(rt.package_root()));
				let selection = build_render_selection(
					&index,
					&[], // No explicit search results here, we're using filter
					true,
					full_source_ids,
				);
				renderer = renderer.with_selection(selection);
			}

			let mut rendered = renderer.render(&crate_data)?;

			if !raw_files_content.is_empty() {
				rendered = format!("{}\n---\n\n{}", raw_files_content, rendered);
			}

			if let Some(ref name) = rt.package_name {
				let header = match self.render_format {
					RenderFormat::Markdown => format!("# Package: {name}\n\n"),
					RenderFormat::Rust => format!("// Package: {name}\n\n"),
				};
				rendered = format!("{header}{rendered}");
			}

			if !rendered.trim().is_empty() {
				rendered_outputs.push(rendered);
			}
		}

		let separator = match self.render_format {
			RenderFormat::Markdown => "\n\n---\n\n",
			RenderFormat::Rust => {
				"\n\n// ----------------------------------------------------------------------------\n\n"
			}
		};

		Ok(rendered_outputs.join(separator))
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
		let crates = self.inspect(
			target,
			no_default_features,
			all_features,
			features,
			private_items,
		)?;

		if crates.len() == 1 {
			Ok(serde_json::to_string_pretty(&crates[0])?)
		} else {
			Ok(serde_json::to_string_pretty(&crates)?)
		}
	}
}
