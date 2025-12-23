use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core_api::Result;

/// State of an ongoing skeleton build.
#[derive(Serialize, Deserialize, Debug)]
pub struct SkeleState {
	/// Path to the output file where skeletonized code is written.
	pub output_path: Option<PathBuf>,
	/// List of entries (targets or manual injections) in the skeleton.
	pub entries: Vec<SkeleEntry>,
	/// Whether to use plain output (skip module nesting). Defaults to true.
	#[serde(default = "default_plain")]
	pub plain: bool,
}

fn default_plain() -> bool {
	true
}

impl Default for SkeleState {
	fn default() -> Self {
		Self {
			output_path: None,
			entries: Vec::new(),
			plain: true,
		}
	}
}

/// An entry in the skeleton build.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkeleEntry {
	/// A target to be rendered from a crate.
	Target(SkeleTarget),
	/// A manual text injection.
	Injection(SkeleInjection),
	/// A raw source snippet loaded directly from disk.
	RawSource(SkeleRawSource),
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
	/// Whether to search private items when resolving this target. Defaults to true.
	#[serde(default = "default_private")]
	pub private: bool,
}

fn default_private() -> bool {
	true
}

/// A manual text injection.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SkeleInjection {
	/// The text to inject.
	pub content: String,
}

/// A raw source snippet loaded directly from disk.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SkeleRawSource {
	/// Absolute path to the file.
	pub file: PathBuf,
	/// Canonical repo-root-relative path (POSIX style with forward slashes).
	/// This is the stable match key used for lookups.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub canonical_key: Option<String>,
	/// 1-based inclusive start line, if set.
	#[serde(default)]
	pub start_line: Option<usize>,
	/// 1-based inclusive end line, if set.
	#[serde(default)]
	pub end_line: Option<usize>,
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
		/// Whether to search private items.
		private: bool,
		/// Strict mode: disable heuristics during validation.
		#[allow(dead_code)]
		strict: bool,
	},
	/// Add multiple targets in one operation.
	AddMany {
		/// Target paths to add.
		targets: Vec<String>,
		/// Whether to include elided source implementation.
		implementation: bool,
		/// Whether to include literal, unelided source.
		raw_source: bool,
		/// Whether to validate the targets before saving.
		validate: bool,
		/// Whether to search private items.
		private: bool,
		/// Strict mode: disable heuristics during validation.
		#[allow(dead_code)]
		strict: bool,
	},
	/// Add a raw source snippet from disk.
	AddRaw {
		/// Raw source spec: `/path/to/file.rs[:start[:end]]` (1-based lines).
		spec: String,
	},
	/// Add multiple raw source snippets from disk.
	AddRawMany {
		/// Raw source specs: `/path/to/file.rs[:start[:end]]` (1-based lines).
		specs: Vec<String>,
	},
	/// Add targets and raw snippets derived from a git diff.
	AddChangedResolved {
		/// Target specs to add.
		targets: Vec<String>,
		/// Raw source specs to add.
		raw_specs: Vec<String>,
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
	Status {
		/// Show keys in machine-parsable format.
		keys: bool,
	},
	/// Preview the output to stdout.
	Preview,
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
