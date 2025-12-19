use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core_api::search::{SearchDomain, SearchIndex, SearchOptions, build_render_selection};
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
					let resolved = resolve_target(&t.path, false)?;
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
						.with_current_file(last_file.clone());

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
		/// Optional target path to inject after.
		after: Option<String>,
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
		Some(SkeleAction::Inject { content, after }) => {
			let injection = SkeleEntry::Injection(SkeleInjection { content });
			if let Some(after_path) = after {
				if let Some(pos) = state.entries.iter().position(|e| match e {
					SkeleEntry::Target(t) => t.path == after_path,
					_ => false,
				}) {
					state.entries.insert(pos + 1, injection);
					println!("Injected commentary after {}", after_path);
				} else {
					state.entries.push(injection);
					println!("Target {} not found; injected at end.", after_path);
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
			state = SkeleState::default();
			if let Some(ref out) = output {
				state.output_path = Some(out.clone());
			}
			state.flat = flat;
			println!("State reset.");
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
	println!("  Output: {}", output_path.display());
	println!("  Flat: {}", state.flat);
	println!("  Entries:");
	for e in &state.entries {
		match e {
			SkeleEntry::Target(t) => println!("    - [Target] {} (full: {})", t.path, t.full),
			SkeleEntry::Injection(i) => {
				let summary = if i.content.len() > 40 {
					format!("{}...", &i.content[..37])
				} else {
					i.content.clone()
				};
				println!("    - [Inject] {}", summary.replace('\n', "\\n"));
			}
		}
	}

	Ok(())
}
