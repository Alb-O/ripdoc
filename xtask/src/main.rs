use clap::Parser;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const FIXTURE_SUBDIR: &str = "crates/libruskel/tests/fixtures";
const COMMAND: [&str; 7] = [
	"rustdoc",
	"-Z",
	"unstable-options",
	"--output-format",
	"json",
	"--",
	"--document-private-items",
];
const CARGO_TOML: &str = "Cargo.toml";
const RUSTDOC_JSON: &str = "rustdoc.json";
const DOC_DIR: &str = "target/doc";

#[derive(Parser, Debug)]
#[command(name = "xtask")]
enum Cli {
	/// Regenerate rustdoc.json fixtures for all fixture directories
	RegenerateFixtures,
}

fn main() {
	let args = Cli::parse();
	match args {
		Cli::RegenerateFixtures => regenerate_fixtures(),
	}
}

fn regenerate_fixtures() {
	let project_root = get_project_root();
	let fixtures_dir = project_root.join(FIXTURE_SUBDIR);

	if !fixtures_dir.exists() {
		panic!("Fixtures directory not found at {}", fixtures_dir.display());
	}

	for entry in fs::read_dir(&fixtures_dir).unwrap() {
		let entry = entry.unwrap();
		let path = entry.path();

		if !path.is_dir() || !path.join(CARGO_TOML).exists() {
			continue;
		}

		let fixture_name = path.file_name().unwrap().to_str().unwrap();
		print!("{}... ", fixture_name);

		let output = Command::new("cargo")
			.args(&COMMAND)
			.current_dir(&path)
			.output()
			.unwrap();

		if !output.status.success() {
			panic!("Command failed for fixture: {}", fixture_name);
		}

		let doc_dir = path.join(DOC_DIR);
		let json_file = fs::read_dir(&doc_dir)
			.unwrap()
			.find_map(|entry| {
				let entry = entry.ok()?;
				let path = entry.path();
				if path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false) {
					Some(path)
				} else {
					None
				}
			});

		let json_path = json_file.expect("No rustdoc json file found");
		let dest = path.join(RUSTDOC_JSON);
		fs::copy(&json_path, &dest).unwrap();
		println!("OK");
	}
}

fn get_project_root() -> PathBuf {
	let mut current = env::current_dir().unwrap();
	loop {
		if current.join(CARGO_TOML).exists() && current.join("xtask").exists() {
			return current;
		}
		if !current.pop() {
			panic!("Could not find project root");
		}
	}
}
