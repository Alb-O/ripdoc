mod rebuild;
/// Target resolution and validation logic.
pub mod resolver;
/// Persistent state and data structures for skelebuild.
pub mod state;

use std::path::PathBuf;

pub use resolver::unescape_inject_content;
use resolver::{
	find_target_match, normalize_target_spec_for_storage, validate_add_target_or_error,
};
pub use state::{SkeleAction, SkeleEntry, SkeleInjection, SkeleRawSource, SkeleState, SkeleTarget};

use crate::core_api::error::RipdocError;
use crate::core_api::{Result, Ripdoc};

pub(crate) enum SkeleGroup {
	Targets {
		pkg_root: PathBuf,
		targets: Vec<SkeleTarget>,
	},
	Injection(String),
	RawSource(SkeleRawSource),
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
	let prev_output_path = state.output_path.clone();
	let prev_plain = state.plain;

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

	let config_changed = state.output_path != prev_output_path || state.plain != prev_plain;
	let show_state_on_exit = show_state || matches!(action.as_ref(), Some(SkeleAction::Status));
	let mut action_summary: Option<String> = None;

	let mut should_rebuild = false;
	match action {
		Some(SkeleAction::Add {
			target,
			implementation,
			raw_source,
			validate,
		}) => {
			let normalized_target = normalize_target_spec_for_storage(&target);
			let validated = if validate {
				Some(validate_add_target_or_error(&normalized_target, ripdoc)?)
			} else {
				None
			};

			let already_present = state.entries.iter().any(|e| match e {
				SkeleEntry::Target(t) => t.path == normalized_target,
				_ => false,
			});

			should_rebuild = config_changed;
			if already_present {
				action_summary = Some(format!(
					"No change (target already exists): {normalized_target}"
				));
			} else {
				state.entries.push(SkeleEntry::Target(SkeleTarget {
					path: normalized_target.clone(),
					implementation,
					raw_source,
				}));
				let index = state.entries.len() - 1;
				let source = validated
					.as_ref()
					.and_then(|info| info.source_location.as_deref())
					.unwrap_or("-");
				let span_lines = validated
					.as_ref()
					.and_then(|info| info.span_line_count)
					.map(|count| count.to_string())
					.unwrap_or_else(|| "-".to_string());
				should_rebuild = true;
				action_summary = Some(format!(
					"Added target: {normalized_target} (entry #{index}, source: {source}, span_lines: {span_lines}, implementation: {}, raw_source: {})",
					if implementation { "yes" } else { "no" },
					if raw_source { "yes" } else { "no" }
				));
			}
		}
		Some(SkeleAction::AddMany {
			targets,
			implementation,
			raw_source,
			validate,
		}) => {
			let mut added: Vec<String> = Vec::new();
			let mut added_indices: Vec<usize> = Vec::new();
			let mut already: Vec<String> = Vec::new();
			for target in targets {
				let normalized_target = normalize_target_spec_for_storage(&target);
				if validate {
					let _ = validate_add_target_or_error(&normalized_target, ripdoc)?;
				}
				let is_present = state.entries.iter().any(|e| match e {
					SkeleEntry::Target(t) => t.path == normalized_target,
					_ => false,
				});
				if is_present {
					already.push(normalized_target);
					continue;
				}
				added.push(normalized_target.clone());
				state.entries.push(SkeleEntry::Target(SkeleTarget {
					path: normalized_target,
					implementation,
					raw_source,
				}));
				added_indices.push(state.entries.len() - 1);
			}

			should_rebuild = config_changed || !added.is_empty();
			if added.is_empty() {
				action_summary = Some(format!(
					"No change (all targets already exist): {}",
					already.len()
				));
			} else {
				let indices = if added_indices.len() <= 8 {
					added_indices
						.iter()
						.map(|idx| format!("#{idx}"))
						.collect::<Vec<_>>()
						.join(", ")
				} else {
					format!(
						"#{}..#{}",
						added_indices.first().unwrap_or(&0),
						added_indices.last().unwrap_or(&0)
					)
				};
				action_summary = Some(format!(
					"Added {} targets (entries: {indices}, implementation: {}, raw_source: {})",
					added.len(),
					if implementation { "yes" } else { "no" },
					if raw_source { "yes" } else { "no" }
				));
			}
		}
		Some(SkeleAction::AddRaw { spec }) => {
			let raw = parse_raw_source_spec(&spec)?;
			let already_present = state.entries.iter().any(|e| match e {
				SkeleEntry::RawSource(existing) => existing == &raw,
				_ => false,
			});

			should_rebuild = config_changed;
			if already_present {
				action_summary = Some(format!(
					"No change (raw source already exists): {}",
					raw_source_summary(&raw)
				));
			} else {
				state.entries.push(SkeleEntry::RawSource(raw.clone()));
				let index = state.entries.len() - 1;
				should_rebuild = true;
				action_summary = Some(format!(
					"Added raw source: {} (entry #{index})",
					raw_source_summary(&raw)
				));
			}
		}
		Some(SkeleAction::AddRawMany { specs }) => {
			let mut added: Vec<SkeleRawSource> = Vec::new();
			let mut already: Vec<SkeleRawSource> = Vec::new();
			let mut added_indices: Vec<usize> = Vec::new();

			for spec in specs {
				let raw = parse_raw_source_spec(&spec)?;
				let exists = state.entries.iter().any(|e| match e {
					SkeleEntry::RawSource(existing) => existing == &raw,
					_ => false,
				});
				if exists {
					already.push(raw);
					continue;
				}
				added.push(raw.clone());
				state.entries.push(SkeleEntry::RawSource(raw));
				added_indices.push(state.entries.len() - 1);
			}

			should_rebuild = config_changed || !added.is_empty();
			if added.is_empty() {
				action_summary = Some(format!(
					"No change (all raw sources already exist): {}",
					already.len()
				));
			} else {
				let indices = if added_indices.len() <= 8 {
					added_indices
						.iter()
						.map(|idx| format!("#{idx}"))
						.collect::<Vec<_>>()
						.join(", ")
				} else {
					format!(
						"#{}..#{}",
						added_indices.first().unwrap_or(&0),
						added_indices.last().unwrap_or(&0)
					)
				};
				action_summary = Some(format!(
					"Added {} raw sources (entries: {indices})",
					added.len()
				));
			}
		}
		Some(SkeleAction::AddChangedResolved { targets, raw_specs }) => {
			let mut added_targets: Vec<String> = Vec::new();
			let mut already_targets: Vec<String> = Vec::new();
			for target in targets {
				let normalized_target = normalize_target_spec_for_storage(&target);
				let is_present = state.entries.iter().any(|e| match e {
					SkeleEntry::Target(t) => t.path == normalized_target,
					_ => false,
				});
				if is_present {
					already_targets.push(normalized_target);
					continue;
				}
				added_targets.push(normalized_target.clone());
				state.entries.push(SkeleEntry::Target(SkeleTarget {
					path: normalized_target,
					implementation: true,
					raw_source: false,
				}));
			}

			let mut added_raw: Vec<SkeleRawSource> = Vec::new();
			let mut already_raw: Vec<SkeleRawSource> = Vec::new();
			for spec in raw_specs {
				let raw = parse_raw_source_spec(&spec)?;
				let exists = state.entries.iter().any(|e| match e {
					SkeleEntry::RawSource(existing) => existing == &raw,
					_ => false,
				});
				if exists {
					already_raw.push(raw);
					continue;
				}
				added_raw.push(raw.clone());
				state.entries.push(SkeleEntry::RawSource(raw));
			}

			should_rebuild = config_changed || !added_targets.is_empty() || !added_raw.is_empty();
			action_summary = Some(format!(
				"Added changed-context: {} targets ({} already), {} raw snippets ({} already)",
				added_targets.len(),
				already_targets.len(),
				added_raw.len(),
				already_raw.len()
			));
		}
		Some(SkeleAction::Inject {
			content,
			literal,
			after,
			after_target,
			before_target,
			at,
		}) => {
			should_rebuild = true;
			let content = if literal {
				content
			} else {
				unescape_inject_content(&content)
			};
			let injection = SkeleEntry::Injection(SkeleInjection { content });

			let summary = if let Some(index) = at {
				if index > state.entries.len() {
					return Err(RipdocError::InvalidTarget(format!(
						"Invalid --at index {index}; valid range is 0..={}.",
						state.entries.len()
					)));
				}
				state.entries.insert(index, injection);
				format!("Injected commentary at index {index}.")
			} else if let Some(spec) = before_target {
				let index = find_target_match(&state.entries, &spec)?;
				state.entries.insert(index, injection);
				format!("Injected commentary before target at entry #{index}.")
			} else if let Some(spec) = after_target {
				let index = find_target_match(&state.entries, &spec)?;
				let insert_at = index + 1;
				state.entries.insert(insert_at, injection);
				format!("Injected commentary after target at entry #{index}.")
			} else if let Some(after_key) = after {
				let after_key = after_key.trim().to_string();
				let after_upper = after_key.to_uppercase();
				if after_upper == "START" || after_upper == "TOP" || after_upper == "BEGIN" {
					state.entries.insert(0, injection);
					"Injected commentary at entry #0.".to_string()
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
							SkeleEntry::RawSource(r) => {
								let summary = raw_source_summary(r);
								summary == after_key || summary.starts_with(&after_key)
							}
						};
						if is_match {
							matches.push(idx);
						}
					}

					match matches.as_slice() {
						[] => {
							return Err(RipdocError::InvalidTarget(format!(
								"No entry matches --after '{}'. Use `ripdoc skelebuild status` to see entries, then use `inject --after-target/--before-target` or `inject --at <index>`.",
								after_key
							)));
						}
						[only] => {
							let insert_at = only + 1;
							state.entries.insert(insert_at, injection);
							format!("Injected commentary after entry #{only}.")
						}
						_ => {
							return Err(RipdocError::InvalidTarget(format!(
								"Ambiguous --after '{}': matches entries {:?}. Use `inject --after-target/--before-target` with a more specific spec, or `inject --at <index>`.",
								after_key, matches
							)));
						}
					}
				}
			} else {
				state.entries.push(injection);
				let index = state.entries.len() - 1;
				format!("Injected commentary at entry #{index}.")
			};

			action_summary = Some(summary);
		}
		Some(SkeleAction::Update {
			spec,
			implementation,
			raw_source,
		}) => {
			let index = find_target_match(&state.entries, &spec)?;
			let entry = state.entries.get_mut(index).ok_or_else(|| {
				RipdocError::InvalidTarget(format!("Invalid entry index {index}"))
			})?;
			let SkeleEntry::Target(target) = entry else {
				return Err(RipdocError::InvalidTarget(format!(
					"Entry #{index} matched '{spec}' but is not a target",
				)));
			};

			let prev_impl = target.implementation;
			let prev_raw_source = target.raw_source;
			if let Some(value) = implementation {
				target.implementation = value;
			}
			if let Some(value) = raw_source {
				target.raw_source = value;
			}

			let changed =
				target.implementation != prev_impl || target.raw_source != prev_raw_source;
			should_rebuild = config_changed || changed;
			action_summary = Some(if changed {
				format!(
					"Updated target: {} (implementation: {}, raw_source: {})",
					target.path,
					if target.implementation { "yes" } else { "no" },
					if target.raw_source { "yes" } else { "no" }
				)
			} else {
				format!(
					"No change (target already has requested settings): {} (implementation: {}, raw_source: {})",
					target.path,
					if target.implementation { "yes" } else { "no" },
					if target.raw_source { "yes" } else { "no" }
				)
			});
		}
		Some(SkeleAction::Remove(target_str)) => {
			let before_len = state.entries.len();
			state.entries.retain(|e| match e {
				SkeleEntry::Target(t) => t.path != target_str,
				SkeleEntry::Injection(i) => i.content != target_str,
				SkeleEntry::RawSource(r) => {
					raw_source_summary(r) != target_str && r.file.to_string_lossy() != target_str
				}
			});
			let removed = before_len - state.entries.len();
			should_rebuild = config_changed || removed > 0;
			action_summary = Some(if removed > 0 {
				format!("Removed entry: {target_str} (removed: {removed})")
			} else {
				format!("No entries removed for: {target_str}")
			});
		}
		Some(SkeleAction::Reset) => {
			// Preserve output path and plain setting from previous state unless overridden.
			let prev_output = state.output_path.clone();
			let prev_plain = state.plain;
			state = SkeleState::default();
			state.output_path = output.clone().or(prev_output);
			state.plain = plain || prev_plain;
			should_rebuild = true;
			action_summary =
				Some("State reset (entries cleared, output/plain preserved).".to_string());
		}
		Some(SkeleAction::Preview) => {
			let rendered = state.build_output(ripdoc)?;
			print!("{rendered}");
			state.save()?;
			return Ok(());
		}
		Some(SkeleAction::Rebuild) => {
			should_rebuild = true;
			action_summary = Some("Rebuilt output.".to_string());
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
		println!(
			"  State file: {}",
			state::SkeleState::state_file().display()
		);
		println!("  Output: {}", output_path.display());
		println!("  Plain mode: {}", state.plain);
		println!("  Entries: {}", state.entries.len());
		println!(
			"  Tip: prefer `inject --after-target/--before-target` to avoid index shifting; use `inject --at <index>` for precise placement.",
		);
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
				SkeleEntry::RawSource(raw) => {
					println!("    {idx}: [Raw] {}", raw_source_summary(raw));
				}
			}
		}
	} else if let Some(summary) = action_summary {
		println!(
			"{summary} (output: {}, entries: {})",
			output_path.display(),
			state.entries.len()
		);
	} else {
		println!(
			"Output: {} (entries: {})",
			output_path.display(),
			state.entries.len()
		);
	}

	Ok(())
}

fn raw_source_summary(raw: &SkeleRawSource) -> String {
	match (raw.start_line, raw.end_line) {
		(Some(start), Some(end)) if start == end => format!("{}:{start}", raw.file.display()),
		(Some(start), Some(end)) => format!("{}:{start}:{end}", raw.file.display()),
		_ => raw.file.display().to_string(),
	}
}

fn parse_raw_source_spec(spec: &str) -> Result<SkeleRawSource> {
	let trimmed = spec.trim();
	if trimmed.is_empty() {
		return Err(RipdocError::InvalidTarget(
			"Raw source spec is empty".to_string(),
		));
	}

	let (path_part, start_line, end_line) = match trimmed.rsplit_once(':') {
		None => (trimmed, None, None),
		Some((maybe_path, last)) => {
			let Ok(last_num) = last.parse::<usize>() else {
				return Ok(SkeleRawSource {
					file: normalize_file_path(trimmed)?,
					start_line: None,
					end_line: None,
				});
			};
			match maybe_path.rsplit_once(':') {
				Some((path, start)) => match start.parse::<usize>() {
					Ok(start_num) => (path, Some(start_num), Some(last_num)),
					Err(_) => (maybe_path, Some(last_num), Some(last_num)),
				},
				None => (maybe_path, Some(last_num), Some(last_num)),
			}
		}
	};

	let file = normalize_file_path(path_part)?;
	if !file.exists() {
		return Err(RipdocError::InvalidTarget(format!(
			"Raw source file not found: {}",
			file.display()
		)));
	}

	if let (Some(start), Some(end)) = (start_line, end_line) {
		if start == 0 || end == 0 {
			return Err(RipdocError::InvalidTarget(
				"Raw source line numbers are 1-based (must be >= 1)".to_string(),
			));
		}
		if start > end {
			return Err(RipdocError::InvalidTarget(format!(
				"Raw source line range is invalid: start ({start}) > end ({end})",
			)));
		}
	}

	Ok(SkeleRawSource {
		file,
		start_line,
		end_line,
	})
}

fn normalize_file_path(path_str: &str) -> Result<PathBuf> {
	let path = PathBuf::from(path_str);
	let abs = if path.is_relative() {
		std::path::absolute(&path).map_err(|err| {
			RipdocError::InvalidTarget(format!(
				"Failed to resolve raw source path '{}': {err}",
				path.display()
			))
		})?
	} else {
		path
	};
	Ok(abs)
}
