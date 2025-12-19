use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::core_api::{Result, Ripdoc};

/// State of an ongoing skeleton build.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SkeleState {
	/// Path to the output file where skeletonized code is appended.
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
}

/// Executes the skelebuild subcommand.
pub fn run_skelebuild(
	target: Option<String>,
	output: Option<PathBuf>,
	reset: bool,
	ripdoc: &Ripdoc,
	no_default_features: bool,
	all_features: bool,
	features: Vec<String>,
	private_items: bool,
) -> Result<()> {
	let mut state = if reset {
		SkeleState::default()
	} else {
		SkeleState::load()
	};

	if let Some(out) = output {
		state.output_path = Some(out);
	}

	let output_path = state.output_path.clone().unwrap_or_else(|| PathBuf::from("skeleton.rs"));

	if let Some(target_str) = target {
		let rendered = ripdoc.render(
			&target_str,
			no_default_features,
			all_features,
			features,
			private_items,
		)?;

		// Append to the file
		use std::io::Write;
		let mut file = fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(&output_path)?;
		
		writeln!(file, "\n// Target: {}\n{}", target_str, rendered)?;
		
		state.targets.push(target_str);
		println!("Added target to {}", output_path.display());
	} else {
		println!("Current skelebuild state:");
		println!("  Output: {}", output_path.display());
		println!("  Targets: {:?}", state.targets);
	}

	state.save()?;
	Ok(())
}
