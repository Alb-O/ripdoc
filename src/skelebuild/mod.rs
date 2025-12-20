pub mod state;
pub mod resolver;
mod rebuild;

use std::path::PathBuf;
use crate::core_api::error::RipdocError;
use crate::core_api::{Result, Ripdoc};

pub use state::{SkeleState, SkeleEntry, SkeleTarget, SkeleInjection, SkeleAction};
pub use resolver::unescape_inject_content;

use resolver::{
	find_target_match, normalize_target_spec_for_storage, validate_add_target_or_error,
};

pub(crate) enum SkeleGroup {
	Targets {
		pkg_root: PathBuf,
		targets: Vec<SkeleTarget>,
	},
	Injection(String),
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
			validate,
		}) => {
			let normalized_target = normalize_target_spec_for_storage(&target);
			if validate {
				validate_add_target_or_error(&normalized_target, ripdoc)?;
			}
			should_rebuild = true;
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
								"No entry matches --after '{}'. Use `ripdoc skelebuild status` to see entries, then use `inject --after-target/--before-target` or `inject --at <index>`.",
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
								"Ambiguous --after '{}': matches entries {:?}. Use `inject --after-target/--before-target` with a more specific spec, or `inject --at <index>`.",
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
		Some(SkeleAction::Update {
			spec,
			implementation,
			raw_source,
		}) => {
			should_rebuild = true;
			let index = find_target_match(&state.entries, &spec)?;
			let entry = state
				.entries
				.get_mut(index)
				.ok_or_else(|| RipdocError::InvalidTarget(format!("Invalid entry index {index}")))?;
			let SkeleEntry::Target(target) = entry else {
				return Err(RipdocError::InvalidTarget(format!(
					"Entry #{index} matched '{spec}' but is not a target",
				)));
			};
			if let Some(value) = implementation {
				target.implementation = value;
			}
			if let Some(value) = raw_source {
				target.raw_source = value;
			}
			println!(
				"Updated target: {} (implementation: {}, raw_source: {})",
				target.path,
				if target.implementation { "yes" } else { "no" },
				if target.raw_source { "yes" } else { "no" }
			);
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
		println!("  State file: {}", state::SkeleState::state_file().display());
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
			}
		}
	} else {
		println!("Output: {} (entries: {})", output_path.display(), state.entries.len());
	}

	Ok(())
}
