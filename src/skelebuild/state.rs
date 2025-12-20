use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::core_api::Result;

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
		/// Whether to validate the target before saving.
		validate: bool,
	},
	/// Inject manual commentary.
	Inject {
		/// Text to inject.
		content: String,
		/// Treat `\n` / `\t` as literal characters.
		literal: bool,
		/// Optional target path/content prefix to inject after.
		after: Option<String>,
		/// Inject after a matching target entry.
		after_target: Option<String>,
		/// Inject before a matching target entry.
		before_target: Option<String>,
		/// Optional numeric index (0-based) to insert at.
		at: Option<usize>,
	},
	/// Update an existing target entry.
	Update {
		/// Target spec to update (matches like `--after-target`).
		spec: String,
		/// New implementation flag, if provided.
		implementation: Option<bool>,
		/// New raw_source flag, if provided.
		raw_source: Option<bool>,
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
