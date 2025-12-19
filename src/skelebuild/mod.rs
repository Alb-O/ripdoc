use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core_api::search::{SearchDomain, SearchIndex, SearchOptions, build_render_selection};
use crate::core_api::error::RipdocError;
use crate::core_api::{Result, Ripdoc};
use crate::render::Renderer;

/// State of an ongoing skeleton build.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SkeleState {
	/// Path to the output file where skeletonized code is written.
	pub output_path: Option<PathBuf>,
	/// List of entries (targets or manual injections) in the skeleton.
	pub entries: Vec<SkeleEntry>,
	/// Whether to flatten the output (skip module nesting).
	pub flat: bool,
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
	/// Whether to include the full source code.
	pub full: bool,
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
	pub fn rebuild(&self, ripdoc: &Ripdoc) -> Result<()> {
		let output_path = self
			.output_path
			.clone()
			.unwrap_or_else(|| PathBuf::from("skeleton.md"));

		if self.entries.is_empty() {
			fs::write(&output_path, "// No entries in skeleton.\n")?;
			return Ok(());
		}

		// Pre-load all crates to avoid redundant I/O
		let mut crates_data = HashMap::new();
		use crate::cargo_utils::resolve_target;

		// Group sequential targets of the same crate to avoid redundant headers and chopy output.
		let mut grouped_entries: Vec<SkeleGroup> = Vec::new();
		for entry in &self.entries {
			match entry {
				SkeleEntry::Target(t) => {
					let resolved = match resolve_target(&t.path, false) {
						Ok(r) => r,
						Err(_) => continue, // Skip unresolved targets during rebuild
					};
					for rt in resolved {
						let pkg_root = rt.package_root().to_path_buf();
						if !crates_data.contains_key(&pkg_root) {
							let data = rt.read_crate(
								false,
								false,
								vec![],
								true,
								true,
								&crate::cargo_utils::CacheConfig::default(),
							)?;
							crates_data.insert(pkg_root.clone(), data);
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
				SkeleGroup::Targets { pkg_root, targets } => {
					let crate_data = crates_data.get(&pkg_root).unwrap();
					let index = SearchIndex::build(crate_data, true, Some(&pkg_root));

					let mut all_results = Vec::new();
					let mut full_source_ids = std::collections::HashSet::new();

					for t in &targets {
						let mut options = SearchOptions::new(&t.path);
						options.include_private = true;
						options.domains = SearchDomain::PATHS;
						let results = index.search(&options);

						let final_results = if results.is_empty() {
							if let Ok(parsed) = crate::cargo_utils::target::Target::parse(&t.path) {
								let query = parsed.path.join("::");
								if !query.is_empty() {
									let mut fallback_options = SearchOptions::new(&query);
									fallback_options.include_private = true;
									fallback_options.domains = SearchDomain::PATHS;
									fallback_options.case_sensitive = true;
									index.search(&fallback_options)
								} else {
									Vec::new()
								}
							} else {
								Vec::new()
							}
						} else {
							results
						};

						for res in &final_results {
							if t.full && res.kind != crate::core_api::SearchItemKind::Crate {
								full_source_ids.insert(res.item_id);
								if let Some(item) = crate_data.index.get(&res.item_id) {
									match &item.inner {
										rustdoc_types::ItemEnum::Struct(s) => {
											full_source_ids.extend(s.impls.iter().copied());
										}
										rustdoc_types::ItemEnum::Enum(e) => {
											full_source_ids.extend(e.impls.iter().copied());
										}
										rustdoc_types::ItemEnum::Trait(tr) => {
											full_source_ids.extend(tr.items.iter().copied());
										}
										rustdoc_types::ItemEnum::Use(u) => {
											if let Some(id) = &u.id {
												full_source_ids.insert(*id);
												// Recursively find impls for the imported item if it's a struct/enum/trait
												if let Some(imported) = crate_data.index.get(id) {
													match &imported.inner {
														rustdoc_types::ItemEnum::Struct(s) => {
															full_source_ids.extend(s.impls.iter().copied());
														}
														rustdoc_types::ItemEnum::Enum(e) => {
															full_source_ids.extend(e.impls.iter().copied());
														}
														rustdoc_types::ItemEnum::Trait(tr) => {
															full_source_ids.extend(tr.items.iter().copied());
														}
														_ => {}
													}
												}
											}
										}
										_ => {}
									}
								}
							}
							all_results.push(res.clone());
						}
					}

					if all_results.is_empty() {
						continue;
					}

					let selection = build_render_selection(&index, &all_results, true, full_source_ids);
					let renderer = Renderer::default()
						.with_format(ripdoc.render_format())
						.with_private_items(true)
						.with_source_labels(ripdoc.render_source_labels())
						.with_selection(selection)
						.with_source_root(pkg_root.clone())
						.with_flat(self.flat)
						.with_current_file(last_file.clone())
						.with_visited(global_visited.clone());

					let (rendered, final_file) = renderer.render_ext(crate_data)?;
					last_file = final_file;

					if !rendered.trim().is_empty() {
						if !final_output.is_empty() {
							final_output.push_str("\n\n---\n\n");
						}
						final_output.push_str(&rendered);
					}
				}
				SkeleGroup::Injection(content) => {
					if !final_output.is_empty() {
						final_output.push_str("\n\n");
					}
					final_output.push_str(&content);
					final_output.push('\n');
					// We don't reset last_file here because manual comments don't change the source file context
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
		/// Whether to include full source code.
		full: bool,
	},
	/// Inject manual commentary.
	Inject {
		/// Text to inject.
		content: String,
		/// Optional target path/content prefix to inject after.
		after: Option<String>,
		/// Optional numeric index (0-based) to insert at.
		at: Option<usize>,
	},
	/// Remove an entry.
	Remove(String),
	/// Reset state.
	Reset,
	/// Show status.
	Status,
}

/// Executes the skelebuild subcommand.
pub fn run_skelebuild(
	action: Option<SkeleAction>,
	output: Option<PathBuf>,
	flat: bool,
	ripdoc: &Ripdoc,
) -> Result<()> {
	let mut state = SkeleState::load();

	if let Some(ref out) = output {
		state.output_path = Some(out.clone());
	}
	if flat {
		state.flat = true;
	}

	match action {
		Some(SkeleAction::Add { target, full }) => {
			if !state.entries.iter().any(|e| match e {
				SkeleEntry::Target(t) => t.path == target,
				_ => false,
			}) {
				state.entries.push(SkeleEntry::Target(SkeleTarget {
					path: target.clone(),
					full,
				}));
				println!(
					"Added target: {} (full: {})",
					target,
					if full { "yes" } else { "no" }
				);
			}
		}
		Some(SkeleAction::Inject { content, after, at }) => {
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
							SkeleEntry::Target(t) => t.path == after_key || t.path.starts_with(&after_key),
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
			state.entries.retain(|e| match e {
				SkeleEntry::Target(t) => t.path != target_str,
				SkeleEntry::Injection(i) => i.content != target_str,
			});
			println!("Removed entry: {}", target_str);
		}
		Some(SkeleAction::Reset) => {
			// Preserve output path and flat setting from previous state unless overridden
			let prev_output = state.output_path.clone();
			let prev_flat = state.flat;
			state = SkeleState::default();
			state.output_path = output.clone().or(prev_output);
			state.flat = flat || prev_flat;
			println!("State reset (entries cleared, output/flat preserved).");
		}
		Some(SkeleAction::Status) | None => {
			// Just showing status or falling through to rebuild
		}
	}

	state.rebuild(ripdoc)?;
	state.save()?;

	let output_path = state
		.output_path
		.clone()
		.unwrap_or_else(|| PathBuf::from("skeleton.md"));
	println!("Skeleton state:");
	println!("  State file: {}", SkeleState::state_file().display());
	println!("  Output: {}", output_path.display());
	println!("  Flat: {}", state.flat);
	println!("  Entries: {}", state.entries.len());
	println!("  Tip: use `inject --at <index>` to avoid brittle matching.");
	println!("  Entry list:");
	for (idx, e) in state.entries.iter().enumerate() {
		match e {
			SkeleEntry::Target(t) => {
				println!("    {idx}: [Target] {} (full: {})", t.path, t.full)
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

	Ok(())
}
