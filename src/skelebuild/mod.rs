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
	/// List of targets that have been added to the skeleton.
	pub targets: Vec<String>,
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

	/// Rebuilds the skeleton file from scratch using all stored targets.
	pub fn rebuild(&self, ripdoc: &Ripdoc) -> Result<()> {
		let output_path = self
			.output_path
			.clone()
			.unwrap_or_else(|| PathBuf::from("skeleton.md"));

		if self.targets.is_empty() {
			fs::write(&output_path, "// No targets added to skeleton.\n")?;
			return Ok(());
		}

		// Map of package_root -> (resolved_target, sub_targets)
		let mut crate_selections = HashMap::new();

		use crate::cargo_utils::resolve_target;
		for target_str in &self.targets {
			let resolved = resolve_target(target_str, false)?;
			for rt in resolved {
				let key = rt.package_root().to_path_buf();
				let entry = crate_selections
					.entry(key)
					.or_insert_with(|| (rt, Vec::new()));
				entry.1.push(target_str.clone());
			}
		}

		let mut rendered_crates = Vec::new();

		for (rt, targets) in crate_selections.into_values() {
			let crate_data = rt.read_crate(
				false,  // no_default_features
				false,  // all_features
				vec![], // features
				true,   // private_items
				true,   // silent
				&crate::cargo_utils::CacheConfig::default(),
			)?;

			let index = SearchIndex::build(&crate_data, true, Some(rt.package_root()));

			let mut all_results = Vec::new();
			for t in &targets {
				let mut options = SearchOptions::new(t);
				options.include_private = true;
				options.domains = SearchDomain::PATHS;

				let results = index.search(&options);

				// If exact path match failed, try matching without the crate name prefix
				if results.is_empty() {
					if let Ok(parsed) = crate::cargo_utils::target::Target::parse(t) {
						let query = parsed.path.join("::");
						if !query.is_empty() {
							let mut fallback_options = SearchOptions::new(&query);
							fallback_options.include_private = true;
							fallback_options.domains = SearchDomain::PATHS;
							all_results.extend(index.search(&fallback_options));
						}
					}
				} else {
					all_results.extend(results);
				}
			}

			if all_results.is_empty() {
				println!("Warning: No items found for targets: {:?}", targets);
				continue;
			}

			let selection = build_render_selection(&index, &all_results, true);
			let renderer = Renderer::default()
				.with_format(ripdoc.render_format())
				.with_private_items(true)
				.with_source_labels(ripdoc.render_source_labels())
				.with_selection(selection);

			let mut rendered = renderer.render(&crate_data)?;

			if let Some(ref name) = rt.package_name {
				let header = match ripdoc.render_format() {
					crate::RenderFormat::Markdown => format!("# Package: {name}\n\n"),
					crate::RenderFormat::Rust => format!("// Package: {name}\n\n"),
				};
				rendered = format!("{header}{rendered}");
			}

			rendered_crates.push(rendered);
		}

		let final_output = rendered_crates.join("\n\n// ---\n\n");
		fs::write(&output_path, final_output)?;

		Ok(())
	}
}

/// Action to perform on the skelebuild state.
pub enum SkeleAction {
	/// Add a target.
	Add(String),
	/// Remove a target.
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
	ripdoc: &Ripdoc,
) -> Result<()> {
	let mut state = SkeleState::load();

	if let Some(ref out) = output {
		state.output_path = Some(out.clone());
	}

	match action {
		Some(SkeleAction::Add(target_str)) => {
			if !state.targets.contains(&target_str) {
				state.targets.push(target_str.clone());
				println!("Added target: {}", target_str);
			}
		}
		Some(SkeleAction::Remove(target_str)) => {
			state.targets.retain(|t| t != &target_str);
			println!("Removed target: {}", target_str);
		}
		Some(SkeleAction::Reset) => {
			state = SkeleState::default();
			if let Some(ref out) = output {
				state.output_path = Some(out.clone());
			}
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
	println!("  Targets: {:?}", state.targets);

	Ok(())
}
