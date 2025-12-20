use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::cargo_utils::resolve_target;
use crate::core_api::search::{SearchResult, SearchIndex, build_render_selection, SearchItemKind};
use crate::core_api::{Result, Ripdoc};
use crate::render::Renderer;

use super::state::{SkeleEntry, SkeleRawSource, SkeleState};
use super::resolver::{resolve_best_path_match, resolve_impl_target};
use super::SkeleGroup;

pub fn ensure_markdown_block_sep(out: &mut String) {
	if out.is_empty() {
		return;
	}
	if out.ends_with("\n\n") {
		return;
	}
	if out.ends_with('\n') {
		out.push('\n');
	} else {
		out.push_str("\n\n");
	}
}

fn render_raw_source(out: &mut String, raw: &SkeleRawSource) -> Result<()> {
	let content = fs::read_to_string(&raw.file)?;
	let lines: Vec<&str> = content.lines().collect();

	let (start_line, end_line) = match (raw.start_line, raw.end_line) {
		(Some(start), Some(end)) => (start, end),
		_ => (1usize, lines.len().max(1)),
	};

	let total_lines = lines.len();
	if total_lines == 0 {
		out.push_str(&format!("### Raw source: {}\n\n```rust\n```\n", raw.file.display()));
		return Ok(());
	}

	if start_line == 0 || end_line == 0 {
		return Err(crate::core_api::error::RipdocError::InvalidTarget(
			"Raw source line numbers are 1-based (must be >= 1)".to_string(),
		));
	}
	if start_line > end_line {
		return Err(crate::core_api::error::RipdocError::InvalidTarget(format!(
			"Raw source line range is invalid: start ({start_line}) > end ({end_line})",
		)));
	}
	if start_line > total_lines {
		return Err(crate::core_api::error::RipdocError::InvalidTarget(format!(
			"Raw source start line {start_line} exceeds file length ({total_lines} lines): {}",
			raw.file.display(),
		)));
	}
	let end_line = end_line.min(total_lines);

	out.push_str(&format!(
		"### Raw source: {}:{start_line}:{end_line}\n\n",
		raw.file.display()
	));
	out.push_str("```rust\n");
	for (idx, line) in lines[(start_line - 1)..end_line].iter().enumerate() {
		out.push_str(line);
		if idx + 1 != end_line - (start_line - 1) {
			out.push('\n');
		}
	}
	out.push_str("\n```\n");
	Ok(())
}

impl SkeleState {
	/// Build the final markdown output without writing it.
	pub fn build_output(&self, ripdoc: &Ripdoc) -> Result<String> {
		// Pre-load all crates to avoid redundant work.
		let mut crates_data: HashMap<PathBuf, rustdoc_types::Crate> = HashMap::new();

		// Group sequential targets of the same crate to avoid redundant headers and choppy output.
		let mut grouped_entries: Vec<SkeleGroup> = Vec::new();
		let mut had_errors = false;
		for entry in &self.entries {
			match entry {
				SkeleEntry::Target(t) => {
					let resolved = match resolve_target(&t.path, ripdoc.offline()) {
						Ok(r) => r,
						Err(err) => {
							had_errors = true;
							eprintln!("Error: failed to resolve target `{}`: {err}", t.path);
							continue;
						}
					};
					for rt in resolved {
						let pkg_root = rt.package_root().to_path_buf();
						if !crates_data.contains_key(&pkg_root) {
							match rt.read_crate(
								false,
								false,
								vec![],
								true,
								ripdoc.silent(),
								ripdoc.cache_config(),
							) {
								Ok(data) => {
									crates_data.insert(pkg_root.clone(), data);
								}
								Err(err) => {
									had_errors = true;
									eprintln!("Error: failed to load crate for `{}`: {err}", t.path);
									continue;
								}
							}
						}

						if let Some(SkeleGroup::Targets {
							pkg_root: last_root,
							targets,
						}) = grouped_entries.last_mut()
						{
							if *last_root == pkg_root {
								targets.push(t.clone());
								continue;
							}
						}
						grouped_entries.push(SkeleGroup::Targets {
							pkg_root: pkg_root.clone(),
							targets: vec![t.clone()],
						});
					}
				}
				SkeleEntry::Injection(i) => {
					grouped_entries.push(SkeleGroup::Injection(i.content.clone()));
				}
				SkeleEntry::RawSource(raw) => {
					grouped_entries.push(SkeleGroup::RawSource(raw.clone()));
				}
			}
		}

		let mut final_output = String::new();
		let mut last_file: Option<PathBuf> = None;

		for group in grouped_entries {
			match group {
				SkeleGroup::Injection(content) => {
					ensure_markdown_block_sep(&mut final_output);
					final_output.push_str(&content);
					ensure_markdown_block_sep(&mut final_output);
				}
				SkeleGroup::RawSource(raw) => {
					ensure_markdown_block_sep(&mut final_output);
					render_raw_source(&mut final_output, &raw)?;
					ensure_markdown_block_sep(&mut final_output);
				}
				SkeleGroup::Targets { pkg_root, targets } => {
					ensure_markdown_block_sep(&mut final_output);
					let crate_data = crates_data.get(&pkg_root).unwrap();
					let mut full_source = HashSet::new();
					let mut raw_files = HashSet::new();
					let mut selection_results: Vec<SearchResult> = Vec::new();

					let index = SearchIndex::build(crate_data, true, Some(&pkg_root));
					let crate_name = crate_data
						.index
						.get(&crate_data.root)
						.and_then(|root| root.name.clone());

					let resolve_span_path = |span: &rustdoc_types::Span| -> PathBuf {
						let mut path = span.filename.clone();
						if path.is_relative() {
							let joined = pkg_root.join(&path);
							if joined.exists() {
								path = joined;
							} else {
								let mut components = span.filename.components();
								while components.next().is_some() {
									let candidate = pkg_root.join(components.as_path());
									if candidate.exists() {
										path = candidate;
										break;
									}
								}
							}
						}
						path.canonicalize().unwrap_or(path)
					};

					let is_local = |result: &SearchResult| -> bool {
						let Some(item) = crate_data.index.get(&result.item_id) else {
							return false;
						};
						let Some(span) = &item.span else {
							return false;
						};
						resolve_span_path(span).starts_with(&pkg_root)
					};

					for target in targets {
						let parsed = crate::cargo_utils::target::Target::parse(&target.path);
						let base_query = match parsed {
							Ok(parsed) => match parsed.entrypoint {
								crate::cargo_utils::target::Entrypoint::Name { name, .. } => {
									if parsed.path.is_empty() {
										name
									} else {
										format!("{name}::{}", parsed.path.join("::"))
									}
								}
								crate::cargo_utils::target::Entrypoint::Path(_) => parsed.path.join("::"),
							},
							Err(_) => String::new(),
						};

						if base_query.is_empty() {
							let flag = if target.raw_source {
								"--raw-source"
							} else if target.implementation {
								"--implementation"
							} else {
								"target"
							};
							eprintln!(
								"Warning: {flag} needs an item path: `{}`",
								target.path
							);
							continue;
						}

						let base = match resolve_best_path_match(
							&index,
							crate_name.as_deref(),
							&pkg_root,
							&base_query,
							&is_local,
						) {
							Some(base) => base,
							None => {
								// Support targeting an entire impl block via `Type::Trait`.
								if let Some((ty_match, impl_id)) = resolve_impl_target(
									&index,
									crate_data,
									crate_name.as_deref(),
									&pkg_root,
									&base_query,
									&is_local,
								) {
									selection_results.push(ty_match);
									full_source.insert(impl_id);
									continue;
								}
								eprintln!(
									"Warning: no matches found for: `{}`",
									base_query
								);
								continue;
							}
						};

						selection_results.push(base.clone());

						if target.raw_source {
							if let Some(item) = crate_data.index.get(&base.item_id)
								&& let Some(span) = &item.span
							{
								raw_files.insert(span.filename.clone());
							}
						}

						if target.implementation {
							if matches!(base.kind, SearchItemKind::Function | SearchItemKind::Method) {
								full_source.insert(base.item_id);
							} else {
								// Prefer full impl blocks when available: individual method spans can sometimes
								// point at the surrounding `impl` item, and the renderer will reject them.
								if let Some(item) = crate_data.index.get(&base.item_id) {
									let impl_ids: Vec<rustdoc_types::Id> = match &item.inner {
										rustdoc_types::ItemEnum::Struct(struct_) => struct_.impls.clone(),
										rustdoc_types::ItemEnum::Enum(enum_) => enum_.impls.clone(),
										rustdoc_types::ItemEnum::Union(union_) => union_.impls.clone(),
										rustdoc_types::ItemEnum::Trait(trait_) => trait_.implementations.clone(),
										_ => Vec::new(),
									};
									for impl_id in impl_ids {
										if let Some(impl_item) = crate_data.index.get(&impl_id)
											&& let Some(span) = &impl_item.span
											&& resolve_span_path(span).starts_with(&pkg_root)
										{
											full_source.insert(impl_id);
										}
									}
								}

								let prefix = format!("{}::", base.path_string);
								for entry in index.entries() {
									if !entry.path_string.starts_with(&prefix) {
										continue;
									}
									if !is_local(entry) {
										continue;
									}
									selection_results.push(entry.clone());
									if matches!(
										entry.kind,
										SearchItemKind::Function | SearchItemKind::Method
									) {
										full_source.insert(entry.item_id);
									}
								}
							}
						}
					}

					// Append raw files first if any.
					let mut wrote_raw_files = false;
					for file_path in raw_files {
						let abs_path = if file_path.is_absolute() {
							file_path.clone()
						} else {
							pkg_root.join(&file_path)
						};
						match fs::read_to_string(&abs_path) {
							Ok(content) => {
								wrote_raw_files = true;
								final_output.push_str(&format!(
									"// ripdoc:source: {}\n\n{}\n\n",
									file_path.display(),
									content
								));
							}
							Err(err) => {
								had_errors = true;
								eprintln!(
									"Error: source not found at `{}`: {err}",
									abs_path.display()
								);
							}
						}
					}

					let mut search_results = selection_results;
					let mut seen = HashSet::new();
					search_results.retain(|r| seen.insert(r.item_id));

					if search_results.is_empty() && full_source.is_empty() && !wrote_raw_files {
						eprintln!("Warning: no renderable targets found in this section.");
					}

					let selection = build_render_selection(
						&index,
						&search_results,
						true,
						full_source,
					);

					let renderer = Renderer::new()
						.with_format(crate::render::RenderFormat::Markdown)
						.with_selection(selection)
						.with_source_root(pkg_root.clone())
						.with_plain(self.plain)
						.with_current_file(last_file.clone());

					let (rendered, final_file) = renderer.render_ext(crate_data)?;
					last_file = final_file;
					final_output.push_str(&rendered);
				}
			}
		}

		if had_errors {
			eprintln!("Completed with errors; output may be incomplete.");
		}
		Ok(final_output)
	}

	/// Rebuilds the skeleton file from scratch using all stored entries.
	pub fn rebuild(&self, ripdoc: &Ripdoc) -> Result<()> {
		let output_path = self
			.output_path
			.clone()
			.unwrap_or_else(|| PathBuf::from("skeleton.md"));
		let output = self.build_output(ripdoc)?;
		fs::write(&output_path, output)?;
		Ok(())
	}
}
