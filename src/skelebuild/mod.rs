use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cargo_utils::resolve_target;
use crate::core_api::error::RipdocError;
use crate::core_api::search::{
	SearchDomain, SearchIndex, SearchItemKind, SearchOptions, SearchResult,
	build_render_selection,
};
use crate::core_api::{Result, Ripdoc};
use crate::render::Renderer;

fn normalize_target_spec_for_storage(target: &str) -> String {
	let parsed = crate::cargo_utils::target::Target::parse(target);
	let Ok(parsed) = parsed else {
		return target.to_string();
	};
	match parsed.entrypoint {
		crate::cargo_utils::target::Entrypoint::Path(path) => {
			let abs = if path.is_relative() {
				match std::path::absolute(&path) {
					Ok(abs) => abs,
					Err(_) => return target.to_string(),
				}
			} else {
				path
			};
			let mut spec = abs.to_string_lossy().to_string();
			if !parsed.path.is_empty() {
				spec.push_str("::");
				spec.push_str(&parsed.path.join("::"));
			}
			spec
		}
		crate::cargo_utils::target::Entrypoint::Name { .. } => target.to_string(),
	}
}

fn target_entry_matches_spec(stored_target: &str, spec: &str) -> bool {
	let spec = spec.trim();
	if spec.is_empty() {
		return false;
	}

	if stored_target == spec {
		return true;
	}

	// For path-based entries stored as "/abs/path/to/crate::item::path",
	// treat `spec` as an item-path matcher by default.
	let stored_item = stored_target
		.split_once("::")
		.map(|(_, item)| item)
		.unwrap_or(stored_target);

	stored_item == spec || stored_item.ends_with(&format!("::{spec}")) || stored_item.contains(spec)
}

fn find_target_match(entries: &[SkeleEntry], spec: &str) -> Result<usize> {
	let mut matches: Vec<usize> = Vec::new();
	for (idx, entry) in entries.iter().enumerate() {
		let SkeleEntry::Target(t) = entry else {
			continue;
		};
		if target_entry_matches_spec(&t.path, spec) {
			matches.push(idx);
		}
	}

	match matches.as_slice() {
		[] => Err(RipdocError::InvalidTarget(format!(
			"No target matches '{spec}'. Use `ripdoc skelebuild status` to see indices.",
		))),
		[only] => Ok(*only),
		_ => Err(RipdocError::InvalidTarget(format!(
			"Ambiguous target match '{spec}': matches entries {matches:?}. Use `inject --at <index>`.",
		))),
	}
}

/// State of an ongoing skeleton build.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SkeleState {
	/// Path to the output file where skeletonized code is written.
	pub output_path: Option<PathBuf>,
	/// List of entries (targets or manual injections) in the skeleton.
	pub entries: Vec<SkeleEntry>,
	/// Whether to use plain output (skip module nesting).
	pub plain: bool,
}

/// An entry in the skeleton build.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkeleEntry {
	/// A target to be rendered from a crate.
	Target(SkeleTarget),
	/// A manual text injection.
	Injection(SkeleInjection),
}

/// A target in the skeleton build.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SkeleTarget {
	/// The target path (e.g., "ratatui::widgets::Block").
	pub path: String,
	/// Whether to include the elided source implementation.
	pub implementation: bool,
	/// Whether to include the literal, unelided source code.
	#[serde(default)]
	pub raw_source: bool,
}

/// A manual text injection.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SkeleInjection {
	/// The text to inject.
	pub content: String,
}

impl SkeleState {
	/// Returns the path to the state file in the XDG state directory.
	pub fn state_file() -> PathBuf {
		let mut path = dirs::state_dir().unwrap_or_else(|| {
			let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
			p.push(".local");
			p.push("state");
			p
		});
		path.push("ripdoc");
		path.push("skelebuild.json");
		path
	}

	/// Loads the skelebuild state from the state file.
	pub fn load() -> Self {
		let path = Self::state_file();
		if path.exists() {
			let content = fs::read_to_string(path).unwrap_or_default();
			serde_json::from_str(&content).unwrap_or_default()
		} else {
			Self::default()
		}
	}

	/// Saves the skelebuild state to the state file.
	pub fn save(&self) -> Result<()> {
		let path = Self::state_file();
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent)?;
		}
		let content = serde_json::to_string_pretty(self)?;
		fs::write(path, content)?;
		Ok(())
	}

	/// Rebuilds the skeleton file from scratch using all stored entries.
	pub fn rebuild(&self, _ripdoc: &Ripdoc) -> Result<()> {
		let output_path = self
			.output_path
			.clone()
			.unwrap_or_else(|| PathBuf::from("skeleton.md"));

		// Pre-load all crates to avoid redundant work.
		let mut crates_data: HashMap<PathBuf, rustdoc_types::Crate> = HashMap::new();

		// Group sequential targets of the same crate to avoid redundant headers and choppy output.
		let mut grouped_entries: Vec<SkeleGroup> = Vec::new();
		for entry in &self.entries {
			match entry {
				SkeleEntry::Target(t) => {
					let resolved = match resolve_target(&t.path, false) {
						Ok(r) => r,
						Err(err) => {
							grouped_entries.push(SkeleGroup::Injection(format!(
								"> [!ERROR] Failed to resolve target `{}`: {err}\n",
								t.path
							)));
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
										true,
										&crate::cargo_utils::CacheConfig::default(),
									) {
										Ok(data) => {
											crates_data.insert(pkg_root.clone(), data);
										}
										Err(err) => {
											grouped_entries.push(SkeleGroup::Injection(format!(
												"> [!ERROR] Failed to load crate for `{}`: {err}\n",
												t.path
											)));
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
			}
		}

		let mut final_output = String::new();
		let mut last_file: Option<PathBuf> = None;
		let global_visited =
			std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

		for group in grouped_entries {
			match group {
				SkeleGroup::Injection(content) => {
					final_output.push_str(&content);
					final_output.push_str("\n\n");
				}
				SkeleGroup::Targets { pkg_root, targets } => {
					let crate_data = crates_data.get(&pkg_root).unwrap();
					let mut warnings: Vec<String> = Vec::new();
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
							warnings.push(format!(
								"> [!WARNING] {flag} needs an item path: `{}`",
								target.path
							));
							continue;
						}

						let mut candidates: Vec<String> = vec![base_query.clone()];
						if let Some((first, rest)) = base_query.split_once("::") {
							if let Some(crate_name) = crate_name.as_deref()
								&& first != crate_name
							{
								candidates.push(format!("{crate_name}::{rest}"));
							}
							candidates.push(rest.to_string());
						}
						candidates.dedup();

						let mut resolved: Option<SearchResult> = None;
						for candidate in &candidates {
							let mut options = SearchOptions::new(candidate);
							options.domains = SearchDomain::PATHS;
							let mut results = index.search(&options);
							if candidate.contains("::") {
								results.retain(|r| {
									r.path_string == *candidate
										|| r.path_string.ends_with(&format!("::{candidate}"))
								});
							}

							let mut local: Vec<SearchResult> =
								results.iter().cloned().filter(|r| is_local(r)).collect();
							let pool = if !local.is_empty() {
								&mut local
							} else {
								&mut results
							};
							if pool.is_empty() {
								continue;
							}

							pool.sort_by_key(|r| {
								(
									!is_local(r),
									match r.kind {
										SearchItemKind::Struct
										| SearchItemKind::Enum
										| SearchItemKind::Trait
										| SearchItemKind::TypeAlias
										| SearchItemKind::Function
										| SearchItemKind::Method => 0usize,
										SearchItemKind::Module => 1usize,
										_ => 2usize,
									},
									r.path_string.len(),
								)
							});
							if pool.len() > 1 {
								warnings.push(format!(
									"> [!WARNING] Ambiguous match for `{}`; using `{}`",
									candidate,
									pool[0].path_string
								));
							}
							resolved = Some(pool[0].clone());
							break;
						}

						let Some(base) = resolved else {
							warnings.push(format!(
								"> [!WARNING] No matches found for: `{}`",
								candidates.join("`, `")
							));
							continue;
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
								warnings.push(format!(
									"> [!ERROR] Source not found at `{}`: {err}",
									abs_path.display()
								));
							}
						}
					}

					let mut search_results = selection_results;
					let mut seen = HashSet::new();
					search_results.retain(|r| seen.insert(r.item_id));

					if search_results.is_empty() && full_source.is_empty() && !wrote_raw_files {
						warnings.push(
							"> [!WARNING] No renderable targets found in this section.".to_string(),
						);
					}

					if !warnings.is_empty() {
						final_output.push_str(&warnings.join("\n"));
						final_output.push_str("\n\n");
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
						.with_current_file(last_file.clone())
						.with_visited(global_visited.clone());

					let (rendered, final_file) = renderer.render_ext(crate_data)?;
					last_file = final_file;
					final_output.push_str(&rendered);
				}
			}
		}

		fs::write(&output_path, final_output)?;
		Ok(())
	}
}

enum SkeleGroup {
	Targets {
		pkg_root: PathBuf,
		targets: Vec<SkeleTarget>,
	},
	Injection(String),
}

/// Action to perform on the skelebuild state.
pub enum SkeleAction {
	/// Add a target.
	Add {
		/// Target path to add.
		target: String,
		/// Whether to include elided source implementation.
		implementation: bool,
		/// Whether to include literal, unelided source.
		raw_source: bool,
	},
	/// Inject manual commentary.
	Inject {
		/// Text to inject.
		content: String,
		/// Optional target path/content prefix to inject after.
		after: Option<String>,
		/// Inject after a matching target entry.
		after_target: Option<String>,
		/// Inject before a matching target entry.
		before_target: Option<String>,
		/// Optional numeric index (0-based) to insert at.
		at: Option<usize>,
	},
	/// Remove an entry.
	Remove(String),
	/// Reset state.
	Reset,
	/// Show status.
	Status,
	/// Rebuild output using current entries.
	Rebuild,
}

/// Executes the skelebuild subcommand.
pub fn run_skelebuild(
	action: Option<SkeleAction>,
	output: Option<PathBuf>,
	plain: bool,
	show_state: bool,
	ripdoc: &Ripdoc,
) -> Result<()> {
	let mut state = SkeleState::load();

	if let Some(ref out) = output {
		let out = if out.is_relative() {
			std::path::absolute(out).map_err(|err| {
				RipdocError::InvalidTarget(format!(
					"Failed to resolve output path '{}': {err}",
					out.display()
				))
			})?
		} else {
			out.clone()
		};
		state.output_path = Some(out);
	}
	if plain {
		state.plain = true;
	}

	let show_state_on_exit = show_state || matches!(action.as_ref(), Some(SkeleAction::Status));

	let mut should_rebuild = false;
	match action {
		Some(SkeleAction::Add {
			target,
			implementation,
			raw_source,
		}) => {
			should_rebuild = true;
			let normalized_target = normalize_target_spec_for_storage(&target);
			if !state.entries.iter().any(|e| match e {
				SkeleEntry::Target(t) => t.path == normalized_target,
				_ => false,
			}) {
				state.entries.push(SkeleEntry::Target(SkeleTarget {
					path: normalized_target.clone(),
					implementation,
					raw_source,
				}));
				println!(
					"Added target: {} (implementation: {}, raw_source: {})",
					normalized_target,
					if implementation { "yes" } else { "no" },
					if raw_source { "yes" } else { "no" }
				);
			}
		}
		Some(SkeleAction::Inject {
			content,
			after,
			after_target,
			before_target,
			at,
		}) => {
			should_rebuild = true;
			let injection = SkeleEntry::Injection(SkeleInjection { content });
			if let Some(index) = at {
				if index > state.entries.len() {
					return Err(RipdocError::InvalidTarget(format!(
						"Invalid --at index {index}; valid range is 0..={}.",
						state.entries.len()
					)));
				}
				state.entries.insert(index, injection);
				println!("Injected commentary at index {index}.");
			} else if let Some(spec) = before_target {
				let index = find_target_match(&state.entries, &spec)?;
				state.entries.insert(index, injection);
				println!("Injected commentary before target at entry #{index}.");
			} else if let Some(spec) = after_target {
				let index = find_target_match(&state.entries, &spec)?;
				let insert_at = index + 1;
				state.entries.insert(insert_at, injection);
				println!("Injected commentary after target at entry #{index}.");
			} else if let Some(after_key) = after {
				let after_key = after_key.trim().to_string();
				let after_upper = after_key.to_uppercase();
				if after_upper == "START" || after_upper == "TOP" || after_upper == "BEGIN" {
					state.entries.insert(0, injection);
					println!("Injected commentary at the start.");
				} else {
					let mut matches: Vec<usize> = Vec::new();
					for (idx, entry) in state.entries.iter().enumerate() {
						let is_match = match entry {
							SkeleEntry::Target(t) => {
								t.path == after_key || t.path.starts_with(&after_key)
							}
							SkeleEntry::Injection(i) => {
								i.content == after_key || i.content.starts_with(&after_key)
							}
						};
						if is_match {
							matches.push(idx);
						}
					}

					match matches.as_slice() {
						[] => {
							return Err(RipdocError::InvalidTarget(format!(
								"No entry matches --after '{}'. Use `ripdoc skelebuild status` to see indices, then use `inject --at <index>`.",
								after_key
							)));
						}
						[only] => {
							let insert_at = only + 1;
							state.entries.insert(insert_at, injection);
							println!("Injected commentary after entry #{only}.");
						}
						_ => {
							return Err(RipdocError::InvalidTarget(format!(
								"Ambiguous --after '{}': matches entries {:?}. Use `inject --at <index>`.",
								after_key, matches
							)));
						}
					}
				}
			} else {
				state.entries.push(injection);
				println!("Injected commentary at end.");
			}
		}
		Some(SkeleAction::Remove(target_str)) => {
			should_rebuild = true;
			state.entries.retain(|e| match e {
				SkeleEntry::Target(t) => t.path != target_str,
				SkeleEntry::Injection(i) => i.content != target_str,
			});
			println!("Removed entry: {}", target_str);
		}
		Some(SkeleAction::Reset) => {
			should_rebuild = true;
			// Preserve output path and plain setting from previous state unless overridden
			let prev_output = state.output_path.clone();
			let prev_plain = state.plain;
			state = SkeleState::default();
			state.output_path = output.clone().or(prev_output);
			state.plain = plain || prev_plain;
			println!("State reset (entries cleared, output/plain preserved).");
		}
		Some(SkeleAction::Rebuild) => {
			should_rebuild = true;
		}
		Some(SkeleAction::Status) | None => {
			// Status is read-only and does not rewrite the output file.
		}
	}

	if should_rebuild {
		state.rebuild(ripdoc)?;
	}
	state.save()?;

	let output_path = state
		.output_path
		.clone()
		.unwrap_or_else(|| PathBuf::from("skeleton.md"));

	let show_full_state = show_state_on_exit;
	if show_full_state {
		println!("Skeleton state:");
		println!("  State file: {}", SkeleState::state_file().display());
		println!("  Output: {}", output_path.display());
		println!("  Plain mode: {}", state.plain);
		println!("  Entries: {}", state.entries.len());
		println!("  Tip: use `inject --at <index>` to avoid brittle matching.");
		println!("  Entry list:");
		for (idx, e) in state.entries.iter().enumerate() {
			match e {
				SkeleEntry::Target(t) => {
					println!(
						"    {idx}: [Target] {} (impl: {}, raw: {})",
						t.path, t.implementation, t.raw_source
					)
				}
				SkeleEntry::Injection(i) => {
					let trimmed = i.content.trim();
					let compact = trimmed.replace('\n', "\\n");
					let summary = if compact.len() > 80 {
						format!("{}...", &compact[..77])
					} else {
						compact
					};
					println!("    {idx}: [Inject] {summary}");
				}
			}
		}
	} else {
		println!("Output: {} (entries: {})", output_path.display(), state.entries.len());
	}

	Ok(())
}
