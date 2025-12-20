//! Integration tests for `skelebuild` workflows.

use std::fs;
use std::path::PathBuf;

use ripdoc::Ripdoc;
use ripdoc::core_api::search::{SearchDomain, SearchIndex, SearchItemKind, SearchOptions};
use ripdoc::skelebuild::{SkeleEntry, SkeleInjection, SkeleState, SkeleTarget};
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
		}),
		SkeleEntry::Injection(SkeleInjection {
			// Stored injections are literal; CLI `inject` now unescapes `\\n` by default.
			content: "### Notes\n- first\n- second".to_string(),
		}),
		SkeleEntry::Target(SkeleTarget {
			path: format!("{}::{inherent_save}", crate_dir.display()),
			implementation: true,
			raw_source: false,
		}),
		// Target an entire impl block via `Type::Trait`.
		SkeleEntry::Target(SkeleTarget {
			path: format!("{}::editor::Editor::EditorOps", crate_dir.display()),
			implementation: false,
			raw_source: false,
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
