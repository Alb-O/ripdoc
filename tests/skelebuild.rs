//! Integration tests for `skelebuild` workflows.

use std::fs;
use std::path::PathBuf;

use ripdoc::Ripdoc;
use ripdoc::core_api::search::{SearchDomain, SearchIndex, SearchItemKind, SearchOptions};
use ripdoc::skelebuild::{
	SkeleAction, SkeleEntry, SkeleInjection, SkeleRawSource, SkeleState, SkeleTarget,
};
use tempfile::TempDir;

fn write_bin_crate_fixture() -> TempDir {
	let temp_dir = TempDir::new().expect("tempdir");
	let src_dir = temp_dir.path().join("src");
	fs::create_dir_all(&src_dir).expect("create src/");

	// Bin-only crate: rustdoc crate name will be the bin name ("tome").
	fs::write(
		temp_dir.path().join("Cargo.toml"),
		r#"
[package]
name = "tome-term"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "tome"
path = "src/main.rs"
"#,
	)
	.expect("write Cargo.toml");

	fs::write(
		src_dir.join("main.rs"),
		r#"
pub mod terminal_panel {
    pub struct TerminalState {
        pub ticks: u32,
    }

    impl TerminalState {
        pub fn new() -> Self {
            Self { ticks: 0 }
        }

        pub fn tick(&mut self) -> u32 {
            let _marker = "tick_body_marker";
            let baseline = 41;
            self.ticks += 1;
            baseline + 1
        }
    }
}

pub mod editor {
    pub trait EditorOps {
        fn save(&self) -> String;
    }

    pub struct Editor;

    impl Editor {
        pub fn save(&self) -> String {
            let tag = "inherent_save_body";
            tag.to_string()
        }
    }

    impl EditorOps for Editor {
        fn save(&self) -> String {
            let tag = "trait_save_body";
            tag.to_string()
        }
    }
}

fn main() {}
"#,
	)
	.expect("write main.rs");

	temp_dir
}

fn find_inherent_save_path(crate_dir: &PathBuf) -> String {
	let ripdoc = Ripdoc::new().with_offline(true).with_silent(true);
	let crates = ripdoc
		.inspect(
			crate_dir.to_str().expect("crate dir must be valid utf-8"),
			false,
			false,
			Vec::new(),
			true,
		)
		.expect("inspect fixture crate");
	let crate_data = crates.into_iter().next().expect("crate data");

	let index = SearchIndex::build(&crate_data, true, Some(crate_dir));
	let mut options = SearchOptions::new("save");
	options.domains = SearchDomain::NAMES | SearchDomain::PATHS;
	let results = index.search(&options);

	let mut inherent: Option<String> = None;
	let mut candidates: Vec<String> = Vec::new();

	for result in results {
		if result.kind != SearchItemKind::Method {
			continue;
		}
		if !result.path_string.ends_with("::save") {
			continue;
		}

		candidates.push(result.path_string.clone());

		// Inherent methods are typically `crate::Type::method`.
		if result.path_string.contains("::Editor::save") {
			inherent.get_or_insert(result.path_string);
		}
	}

	let inherent = inherent.unwrap_or_else(|| {
		panic!(
			"failed to find inherent save path; candidates: {:?}",
			candidates
		)
	});

	inherent
}

#[test]
fn skelebuild_realistic_session_produces_detailed_markdown()
-> Result<(), Box<dyn std::error::Error>> {
	let fixture = write_bin_crate_fixture();
	let crate_dir = fixture.path().to_path_buf();

	let out_dir = TempDir::new()?;
	let out_path = out_dir.path().join("out.md");

	let inherent_save = find_inherent_save_path(&crate_dir);

	let mut state = SkeleState::default();
	state.output_path = Some(out_path.clone());
	state.plain = true;
	state.entries = vec![
		SkeleEntry::Injection(SkeleInjection {
			content: "## Intro\nThis is injected commentary.".to_string(),
		}),
		// Users often guess the crate prefix from the package name (tome_term), but for bin crates
		// rustdoc uses the bin name (tome). skelebuild should still resolve this.
		SkeleEntry::Target(SkeleTarget {
			path: format!(
				"{}::tome_term::terminal_panel::TerminalState",
				crate_dir.display()
			),
			implementation: true,
			raw_source: false,
			private: false,
		}),
		SkeleEntry::Injection(SkeleInjection {
			// Stored injections are literal; CLI `inject` now unescapes `\\n` by default.
			content: "### Notes\n- first\n- second".to_string(),
		}),
		SkeleEntry::Target(SkeleTarget {
			path: format!("{}::{inherent_save}", crate_dir.display()),
			implementation: true,
			raw_source: false,
			private: false,
		}),
		// Target an entire impl block via `Type::Trait`.
		SkeleEntry::Target(SkeleTarget {
			path: format!("{}::editor::Editor::EditorOps", crate_dir.display()),
			implementation: false,
			raw_source: false,
			private: false,
		}),
	];

	let ripdoc = Ripdoc::new().with_offline(true).with_silent(true);
	state.rebuild(&ripdoc)?;

	let output = fs::read_to_string(&out_path)?;

	assert!(output.contains("## Intro"));
	assert!(output.contains("This is injected commentary."));

	// Detailed implementation extraction for a bin crate item.
	assert!(output.contains("pub struct TerminalState"));
	assert!(output.contains("fn tick"));
	assert!(output.contains("tick_body_marker"));

	// Distinguish inherent vs trait impl method bodies.
	assert!(output.contains("inherent_save_body"));
	assert!(output.contains("trait_save_body"));

	// Markdown spacing: ensure injected blocks don't jam into the next source header.
	assert!(
		output.contains("This is injected commentary.\n\n### Source:")
			|| output.contains("This is injected commentary.\n\n##")
	);
	assert!(output.contains("- second\n\n")); // list terminator
	assert!(output.contains("\n\n### Source:"));

	Ok(())
}

#[test]
fn skelebuild_canonical_path_matching() -> Result<(), Box<dyn std::error::Error>> {
	use ripdoc::skelebuild::resolver::find_entry_match;

	let fixture = write_bin_crate_fixture();
	let crate_dir = fixture.path().to_path_buf();

	// Create a test file for raw source
	let test_file = crate_dir.join("test.rs");
	fs::write(&test_file, "// test file\n")?;

	// Create a SkeleRawSource with canonical key
	let raw_source = SkeleRawSource {
		file: test_file.clone(),
		canonical_key: Some("test.rs".to_string()),
		start_line: None,
		end_line: None,
	};

	let entries = vec![
		SkeleEntry::Target(SkeleTarget {
			path: "crate::module::Type".to_string(),
			implementation: true,
			raw_source: false,
			private: true,
		}),
		SkeleEntry::RawSource(raw_source),
	];

	// Test 1: Match by canonical key
	let idx = find_entry_match(&entries, "test.rs")?;
	assert_eq!(idx, 1, "Should match raw source by canonical key");

	// Test 2: Match target by path
	let idx = find_entry_match(&entries, "crate::module::Type")?;
	assert_eq!(idx, 0, "Should match target by path");

	// Test 3: Match by absolute path
	let idx = find_entry_match(&entries, test_file.to_str().unwrap())?;
	assert_eq!(idx, 1, "Should match raw source by absolute path");

	Ok(())
}

// ============================================================================
// Tests for canonical key matching (expanded)
// ============================================================================

#[test]
fn skelebuild_canonical_key_normalization() -> Result<(), Box<dyn std::error::Error>> {
	use ripdoc::skelebuild::resolver::find_entry_match;

	let fixture = write_bin_crate_fixture();
	let crate_dir = fixture.path().to_path_buf();

	// Create nested directories
	let nested_dir = crate_dir.join("crates").join("foo").join("src");
	fs::create_dir_all(&nested_dir)?;
	let test_file = nested_dir.join("lib.rs");
	fs::write(&test_file, "// nested test file\n")?;

	let raw_source = SkeleRawSource {
		file: test_file.clone(),
		canonical_key: Some("crates/foo/src/lib.rs".to_string()),
		start_line: None,
		end_line: None,
	};

	let entries = vec![SkeleEntry::RawSource(raw_source)];

	// Test matching with canonical key
	let idx = find_entry_match(&entries, "crates/foo/src/lib.rs")?;
	assert_eq!(idx, 0, "Should match by exact canonical key");

	Ok(())
}

#[test]
fn skelebuild_find_entry_match_error_shows_available_keys() {
	use ripdoc::skelebuild::resolver::find_entry_match;

	let entries = vec![
		SkeleEntry::Target(SkeleTarget {
			path: "crate::module::Type".to_string(),
			implementation: true,
			raw_source: false,
			private: true,
		}),
		SkeleEntry::RawSource(SkeleRawSource {
			file: PathBuf::from("/tmp/test.rs"),
			canonical_key: Some("src/test.rs".to_string()),
			start_line: None,
			end_line: None,
		}),
	];

	// Try to match a non-existent entry
	let result = find_entry_match(&entries, "nonexistent::path");
	assert!(result.is_err());

	let error_msg = result.unwrap_err().to_string();
	// Error should contain available keys
	assert!(
		error_msg.contains("crate::module::Type"),
		"Error should show available target key"
	);
	assert!(
		error_msg.contains("src/test.rs"),
		"Error should show available raw source key"
	);
	assert!(
		error_msg.contains("status"),
		"Error should suggest status command"
	);
}

// ============================================================================
// Tests for find_entry_match with various path formats
// ============================================================================

#[test]
fn skelebuild_find_entry_match_partial_target_path() -> Result<(), Box<dyn std::error::Error>> {
	use ripdoc::skelebuild::resolver::find_entry_match;

	let entries = vec![SkeleEntry::Target(SkeleTarget {
		path: "/home/user/project::crate::module::submodule::Type".to_string(),
		implementation: true,
		raw_source: false,
		private: true,
	})];

	// Should match by just the item path suffix
	let idx = find_entry_match(&entries, "Type")?;
	assert_eq!(idx, 0, "Should match target by last segment");

	let idx = find_entry_match(&entries, "submodule::Type")?;
	assert_eq!(idx, 0, "Should match target by path suffix");

	let idx = find_entry_match(&entries, "module::submodule::Type")?;
	assert_eq!(idx, 0, "Should match target by longer path suffix");

	Ok(())
}

// ============================================================================
// Tests for injection and entry ordering
// ============================================================================

#[test]
fn skelebuild_injection_placement_with_mixed_entries() -> Result<(), Box<dyn std::error::Error>> {
	use ripdoc::skelebuild::resolver::find_entry_match;

	let entries = vec![
		SkeleEntry::Injection(SkeleInjection {
			content: "## Header".to_string(),
		}),
		SkeleEntry::Target(SkeleTarget {
			path: "crate::first::Item".to_string(),
			implementation: true,
			raw_source: false,
			private: true,
		}),
		SkeleEntry::RawSource(SkeleRawSource {
			file: PathBuf::from("/tmp/raw.rs"),
			canonical_key: Some("src/raw.rs".to_string()),
			start_line: Some(1),
			end_line: Some(10),
		}),
		SkeleEntry::Target(SkeleTarget {
			path: "crate::second::Item".to_string(),
			implementation: true,
			raw_source: false,
			private: true,
		}),
	];

	// Injections should NOT be matchable (they don't have stable keys)
	let result = find_entry_match(&entries, "## Header");
	assert!(
		result.is_err(),
		"Injections should not be matchable by content"
	);

	// But targets and raw sources should be
	let idx = find_entry_match(&entries, "first::Item")?;
	assert_eq!(idx, 1);

	let idx = find_entry_match(&entries, "src/raw.rs")?;
	assert_eq!(idx, 2);

	let idx = find_entry_match(&entries, "second::Item")?;
	assert_eq!(idx, 3);

	Ok(())
}

// ============================================================================
// Tests for SkeleState with status --keys simulation
// ============================================================================

#[test]
fn skelebuild_status_keys_format() {
	let entries = vec![
		SkeleEntry::Target(SkeleTarget {
			path: "crate::module::Type".to_string(),
			implementation: true,
			raw_source: false,
			private: true,
		}),
		SkeleEntry::Injection(SkeleInjection {
			content: "## Notes".to_string(),
		}),
		SkeleEntry::RawSource(SkeleRawSource {
			file: PathBuf::from("/home/user/project/src/lib.rs"),
			canonical_key: Some("src/lib.rs".to_string()),
			start_line: None,
			end_line: None,
		}),
	];

	// Simulate --keys output format
	let mut keys_output = Vec::new();
	for (idx, entry) in entries.iter().enumerate() {
		let (entry_type, key) = match entry {
			SkeleEntry::Target(t) => ("target", t.path.as_str()),
			SkeleEntry::RawSource(r) => (
				"raw",
				r.canonical_key
					.as_deref()
					.unwrap_or_else(|| r.file.to_str().unwrap_or("<invalid-path>")),
			),
			SkeleEntry::Injection(_) => ("injection", "<no-key>"),
		};

		if entry_type != "injection" {
			keys_output.push(format!("{}  {}  {}", idx, entry_type, key));
		}
	}

	assert_eq!(
		keys_output.len(),
		2,
		"Should have 2 entries (injections skipped)"
	);
	assert!(keys_output[0].contains("0  target  crate::module::Type"));
	assert!(keys_output[1].contains("2  raw  src/lib.rs"));
}

// ============================================================================
// Tests for SkeleRawSource canonical key
// ============================================================================

#[test]
fn skelebuild_raw_source_with_line_range() {
	let raw = SkeleRawSource {
		file: PathBuf::from("/home/user/project/src/lib.rs"),
		canonical_key: Some("src/lib.rs".to_string()),
		start_line: Some(10),
		end_line: Some(20),
	};

	assert_eq!(raw.canonical_key.as_deref(), Some("src/lib.rs"));
	assert_eq!(raw.start_line, Some(10));
	assert_eq!(raw.end_line, Some(20));
}

#[test]
fn skelebuild_raw_source_without_canonical_key() {
	// Legacy raw sources might not have canonical_key
	let raw = SkeleRawSource {
		file: PathBuf::from("/absolute/path/to/file.rs"),
		canonical_key: None,
		start_line: None,
		end_line: None,
	};

	// Should fallback to file path
	let key = raw
		.canonical_key
		.as_deref()
		.unwrap_or_else(|| raw.file.to_str().unwrap_or("<invalid>"));
	assert_eq!(key, "/absolute/path/to/file.rs");
}

// ============================================================================
// Tests for SkeleAction with strict flag
// ============================================================================

#[test]
fn skelebuild_action_add_with_strict_flag() {
	let action = SkeleAction::Add {
		target: "crate::module::Type".to_string(),
		implementation: true,
		raw_source: false,
		validate: true,
		private: true,
		strict: true,
	};

	match action {
		SkeleAction::Add { strict, .. } => {
			assert!(strict, "Strict flag should be preserved");
		}
		_ => panic!("Expected Add action"),
	}
}

#[test]
fn skelebuild_action_add_many_with_strict_flag() {
	let action = SkeleAction::AddMany {
		targets: vec!["crate::A".to_string(), "crate::B".to_string()],
		implementation: true,
		raw_source: false,
		validate: true,
		private: true,
		strict: false,
	};

	match action {
		SkeleAction::AddMany {
			strict, targets, ..
		} => {
			assert!(!strict, "Strict flag should be false");
			assert_eq!(targets.len(), 2);
		}
		_ => panic!("Expected AddMany action"),
	}
}

// ============================================================================
// Tests for SkeleAction Status with keys
// ============================================================================

#[test]
fn skelebuild_action_status_with_keys() {
	let action = SkeleAction::Status { keys: true };

	match action {
		SkeleAction::Status { keys } => {
			assert!(keys, "Keys flag should be true");
		}
		_ => panic!("Expected Status action"),
	}
}

#[test]
fn skelebuild_action_status_without_keys() {
	let action = SkeleAction::Status { keys: false };

	match action {
		SkeleAction::Status { keys } => {
			assert!(!keys, "Keys flag should be false");
		}
		_ => panic!("Expected Status action"),
	}
}

// ============================================================================
// Tests for resolver helper functions
// ============================================================================

#[test]
fn skelebuild_target_entry_matches_spec_various_formats() {
	use ripdoc::skelebuild::resolver::target_entry_matches_spec;

	let stored = "/home/user/project::crate::module::submodule::MyType";

	// Exact match
	assert!(target_entry_matches_spec(stored, stored));

	// Match by item path only
	assert!(target_entry_matches_spec(
		stored,
		"crate::module::submodule::MyType"
	));

	// Match by suffix
	assert!(target_entry_matches_spec(stored, "MyType"));
	assert!(target_entry_matches_spec(stored, "submodule::MyType"));

	// No match
	assert!(!target_entry_matches_spec(stored, "OtherType"));
	assert!(!target_entry_matches_spec(stored, "wrong::module::MyType"));
}

#[test]
fn skelebuild_unescape_inject_content() {
	use ripdoc::skelebuild::unescape_inject_content;

	// Test newline unescaping
	assert_eq!(unescape_inject_content("line1\\nline2"), "line1\nline2");

	// Test tab unescaping
	assert_eq!(unescape_inject_content("col1\\tcol2"), "col1\tcol2");

	// Test backslash unescaping
	assert_eq!(
		unescape_inject_content("path\\\\to\\\\file"),
		"path\\to\\file"
	);

	// Test mixed
	assert_eq!(
		unescape_inject_content("## Title\\n\\nParagraph with \\t tab"),
		"## Title\n\nParagraph with \t tab"
	);

	// Test carriage return
	assert_eq!(
		unescape_inject_content("line1\\r\\nline2"),
		"line1\r\nline2"
	);

	// Test literal backslash at end
	assert_eq!(unescape_inject_content("trailing\\"), "trailing\\");

	// Test unknown escape sequence (kept as-is)
	assert_eq!(unescape_inject_content("\\x unknown"), "\\x unknown");
}
