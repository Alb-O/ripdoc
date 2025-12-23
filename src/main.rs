//! CLI entrypoint.

use std::error::Error;
use std::io::IsTerminal;
use std::process::{self, Command as ProcessCommand, Stdio};

use clap::{Args, Parser, Subcommand, ValueEnum};
use owo_colors::OwoColorize;
use regex::Regex;
use ripdoc::cargo_utils::{fetch_readme, find_latest_cached_version, resolve_target};
use ripdoc::core_api::pattern::escape_regex_preserving_pipes;
use ripdoc::core_api::search::{SearchIndex, SearchItemKind};
use ripdoc::{RenderFormat, Ripdoc, SearchDomain, SearchOptions, SourceLocation};

#[derive(Debug, Clone, Copy, ValueEnum)]
/// Available search domains accepted by `--search-spec`.
enum SearchSpec {
	/// Match against item names.
	Name,
	/// Match against documentation comments.
	Doc,
	/// Match against canonical module paths.
	Path,
	/// Match against rendered signatures.
	Signature,
}

impl From<SearchSpec> for SearchDomain {
	fn from(spec: SearchSpec) -> Self {
		match spec {
			SearchSpec::Name => Self::NAMES,
			SearchSpec::Doc => Self::DOCS,
			SearchSpec::Path => Self::PATHS,
			SearchSpec::Signature => Self::SIGNATURES,
		}
	}
}

#[derive(Args, Clone)]
struct CommonArgs {
	/// Include auto-implemented traits
	#[arg(short = 'i', long, default_value_t = false)]
	auto_impls: bool,

	/// Include private items
	#[arg(short = 'p', long, default_value_t = false)]
	private: bool,

	/// Disable default features
	#[arg(short = 'n', long, default_value_t = false)]
	no_default_features: bool,

	/// Enable all features
	#[arg(short = 'a', long, default_value_t = false)]
	all_features: bool,

	/// Specify features to enable
	#[arg(short = 'F', long, value_delimiter = ',')]
	features: Vec<String>,

	/// Enable offline mode, ensuring Cargo will not use the network
	#[arg(short = 'o', long, default_value_t = false)]
	offline: bool,

	/// Enable verbose mode, showing cargo output while generating docs
	#[arg(short = 'v', long, default_value_t = false)]
	verbose: bool,

	/// Select the output format (`rust` or `markdown`)
	#[arg(short = 'f', long, value_enum, default_value = "markdown")]
	format: OutputFormat,

	/// Do not inject source filename labels in the output
	#[arg(long, default_value_t = false)]
	no_source_labels: bool,

	/// Disable ANSI colors in CLI output
	#[arg(long, default_value_t = false)]
	no_color: bool,
}

#[derive(Args, Clone)]
struct SearchFilterArgs {
	/// Comma-separated list of search domains (name, doc, signature, path). Defaults to name, doc, signature.
	#[arg(
		long = "search-spec",
		value_delimiter = ',',
		value_name = "DOMAIN[,DOMAIN...]",
		default_value = "name,doc,signature"
	)]
	#[arg(short = 'S')]
	search_spec: Vec<SearchSpec>,

	/// Execute the search in a case sensitive manner.
	#[arg(short = 'c', long, default_value_t = false)]
	search_case_sensitive: bool,

	/// Suppress automatic expansion of matched containers when searching.
	#[arg(short = 'd', long, default_value_t = false)]
	direct_match_only: bool,
}

impl Default for SearchFilterArgs {
	fn default() -> Self {
		Self {
			search_spec: vec![SearchSpec::Name, SearchSpec::Doc, SearchSpec::Signature],
			search_case_sensitive: false,
			direct_match_only: false,
		}
	}
}

#[derive(Args, Clone)]
struct ListArgs {
	/// Target to generate - a directory, file path, or a module name
	#[arg(default_value = "./")]
	target: String,

	/// Optional search query used to filter the listing
	#[arg(short = 's', long)]
	search: Option<String>,

	#[command(flatten)]
	filters: SearchFilterArgs,

	#[command(flatten)]
	common: CommonArgs,
}

#[derive(Args, Clone)]
struct PrintArgs {
	/// Target to generate - a directory, file path, or a module name
	#[arg(default_value = "./")]
	target: String,

	/// Optional item path to print (uses path-search mode).
	#[arg(value_name = "ITEM", conflicts_with = "search")]
	item: Option<String>,

	/// Search query used to filter the printed skeleton
	#[arg(short = 's', long)]
	search: Option<String>,

	/// Include the elided source implementation for matched items.
	#[arg(long, default_value_t = false)]
	implementation: bool,

	/// Include the literal, unelided source code for the containing file.
	#[arg(long, alias = "source", default_value_t = false)]
	raw_source: bool,

	#[command(flatten)]
	filters: SearchFilterArgs,

	#[command(flatten)]
	common: CommonArgs,
}

#[derive(Args, Clone)]
struct ReadmeArgs {
	/// Target to generate - a directory, file path, or a module name
	#[arg(default_value = "./")]
	target: String,

	#[command(flatten)]
	common: CommonArgs,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
/// Optional topic for `ripdoc agents`.
enum AgentsTopic {
	/// Stateful skeleton builder (`ripdoc skelebuild ...`).
	Skelebuild,
}

#[derive(Args, Clone)]
struct AgentsArgs {
	/// Optional agent guide topic.
	#[arg(value_enum)]
	topic: Option<AgentsTopic>,
}

#[derive(Args, Clone)]
/// Arguments for the `skelebuild` subcommand.
struct SkelebuildArgs {
	#[command(subcommand)]
	command: Option<SkelebuildSubcommand>,

	/// Output file for the skeleton.
	#[arg(short = 'O', long)]
	output: Option<std::path::PathBuf>,

	/// Reset the current skelebuild state.
	#[arg(long)]
	reset: bool,

	/// Plain output (skip module nesting).
	#[arg(long, conflicts_with = "no_plain")]
	plain: bool,

	/// Disable plain output (use module nesting).
	#[arg(long = "no-plain", conflicts_with = "plain")]
	no_plain: bool,

	/// Print full skelebuild state after the command.
	#[arg(long = "show-state", default_value_t = false)]
	show_state: bool,

	#[command(flatten)]
	/// Common arguments for configuring Ripdoc.
	common: CommonArgs,
}

#[derive(Subcommand, Clone)]
enum SkelebuildSubcommand {
	/// Add a target to the skeleton.
	Add {
		/// Target to add.
		target: String,

		/// Item paths to add (uses path-search mode when present).
		#[arg(value_name = "ITEM")]
		items: Vec<String>,

		/// Include the elided source implementation for this item (default: true).
		#[arg(long, default_value_t = true)]
		implementation: bool,

		/// Exclude implementation spans (show signatures only).
		#[arg(long = "no-implementation", conflicts_with = "implementation")]
		no_implementation: bool,

		/// Include the literal, unelided source code for the containing file.
		#[arg(short = 's', long, alias = "source", default_value_t = false)]
		raw_source: bool,

		/// Include private items when resolving targets (default: true).
		#[arg(short = 'p', long, default_value_t = true)]
		private: bool,

		/// Exclude private items when resolving targets.
		#[arg(long = "no-private", conflicts_with = "private")]
		no_private: bool,

		/// Disable validation (allows adding even if it won't resolve until later).
		#[arg(long = "no-validate", default_value_t = false)]
		no_validate: bool,

		/// Strict mode: disable all heuristics (no auto-rewriting crate prefixes).
		#[arg(long, default_value_t = false)]
		strict: bool,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,

		/// Plain output (skip module nesting).
		#[arg(long)]
		plain: bool,
	},
	/// Add an arbitrary raw source snippet by file and line range.
	AddRaw {
		/// Raw source spec: `/path/to/file.rs[:start[:end]]` (1-based lines).
		spec: String,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Add an entire file from disk as raw source.
	AddFile {
		/// Path to the file to include.
		file: std::path::PathBuf,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Add changed-context from a git diff (rustdoc items + raw hunks).
	AddChanged {
		/// Git revspec/range to diff (passed to `git diff --name-only`).
		/// Example: `main...HEAD`.
		#[arg(long, value_name = "REVSPEC", conflicts_with = "staged")]
		git: Option<String>,

		/// Use staged changes (`git diff --name-only --cached`).
		#[arg(long, default_value_t = false)]
		staged: bool,

		/// Only include Rust source files (`.rs`).
		#[arg(long, default_value_t = false)]
		only_rust: bool,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Update an existing target entry.
	Update {
		/// Target spec to update (matches like `inject --after-target`).
		spec: String,

		/// Enable implementation extraction for this entry.
		#[arg(long, conflicts_with = "no_implementation")]
		implementation: bool,
		/// Disable implementation extraction for this entry.
		#[arg(long = "no-implementation", conflicts_with = "implementation")]
		no_implementation: bool,

		/// Enable raw-source inclusion for this entry.
		#[arg(long, conflicts_with = "no_raw_source")]
		raw_source: bool,
		/// Disable raw-source inclusion for this entry.
		#[arg(long = "no-raw-source", conflicts_with = "raw_source")]
		no_raw_source: bool,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Inject manual commentary.
	///
	/// Examples:
	///   # Positional content
	///   ripdoc skelebuild inject "## Notes\nMy commentary" --at 0
	///
	///   # From stdin (heredoc)
	///   ripdoc skelebuild inject --at 0 <<'EOF'
	///   ## Notes
	///   My commentary
	///   EOF
	///
	///   # From stdin (pipe)
	///   cat notes.md | ripdoc skelebuild inject --at 0
	///
	///   # From file
	///   ripdoc skelebuild inject --from-file notes.md --at 0
	///
	///   # After a target
	///   ripdoc skelebuild inject "## Context" --after-target crate::module::Type
	Inject {
		/// Text to inject.
		content: Option<String>,

		/// Read injection content from stdin.
		#[arg(long, default_value_t = false, conflicts_with = "from_file")]
		from_stdin: bool,

		/// Read injection content from a file.
		#[arg(long, value_name = "PATH", conflicts_with = "from_stdin")]
		from_file: Option<std::path::PathBuf>,

		/// Treat `\\n` / `\\t` as literal characters.
		#[arg(long, default_value_t = false)]
		literal: bool,

		/// Inject after this entry (target path or injection content prefix).
		#[arg(long, conflicts_with_all = ["at", "after_target", "before_target"])]
		after: Option<String>,

		/// Inject after a matching target (recommended).
		#[arg(long, conflicts_with_all = ["at", "after", "before_target"])]
		after_target: Option<String>,

		/// Inject before a matching target.
		#[arg(long, conflicts_with_all = ["at", "after", "after_target"])]
		before_target: Option<String>,

		/// Inject at this numeric index (0-based, use `status` to see indices).
		#[arg(long, conflicts_with_all = ["after", "after_target", "before_target"])]
		at: Option<usize>,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Remove a target from the skeleton.
	Remove {
		/// Target to remove.
		target: String,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,
	},
	/// Clear all targets and reset state.
	Reset {
		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,

		/// Plain output (skip module nesting).
		#[arg(long)]
		plain: bool,
	},
	/// Show current targets and output path.
	Status {
		/// Show entry keys in a machine-parsable format.
		#[arg(long, default_value_t = false)]
		keys: bool,
	},
	/// Preview the rebuilt output to stdout.
	Preview,
	/// Rebuild the output file without adding anything.
	Rebuild,
}

#[derive(Subcommand, Clone)]
enum Command {
	/// Print a crate skeleton (default).
	Print(PrintArgs),
	/// Produce a structured item listing.
	List(ListArgs),
	/// Emit raw rustdoc JSON.
	Raw(PrintArgs),
	/// Fetch and print the README of the target crate.
	Readme(ReadmeArgs),
	/// Print a dense guide for AI agents.
	///
	/// Also supports topic guides, e.g. `ripdoc agents skelebuild`.
	Agents(AgentsArgs),
	/// Build a skeleton incrementally.
	Skelebuild(SkelebuildArgs),
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
/// Parsed command-line options for the ripdoc CLI.
struct Cli {
	#[command(subcommand)]
	command: Command,
}

/// Ensure the nightly toolchain and rust-docs JSON component are present.
fn check_nightly_toolchain() -> Result<(), String> {
	// First, check if rustup is available
	let rustup_available = ProcessCommand::new("rustup")
		.arg("--version")
		.stderr(Stdio::null())
		.stdout(Stdio::null())
		.status()
		.map(|status| status.success())
		.unwrap_or(false);

	if rustup_available {
		// Check if nightly toolchain is installed via rustup
		let output = ProcessCommand::new("rustup")
			.args(["run", "nightly", "rustc", "--version"])
			.stderr(Stdio::null())
			.output()
			.map_err(|e| format!("Failed to run rustup: {e}"))?;

		if !output.status.success() {
			return Err("ripdoc requires the nightly toolchain to be installed.\nRun: rustup toolchain install nightly".to_string());
		}
	} else {
		// rustup is not available - check for nightly rustc directly
		let output = ProcessCommand::new("rustc")
			.arg("--version")
			.output()
			.map_err(|e| {
				format!(
					"Failed to run rustc: {e}\nEnsure nightly Rust is installed and available in PATH."
				)
			})?;

		if !output.status.success() {
			return Err("ripdoc requires a nightly Rust toolchain.\nEnsure nightly Rust is installed and available in PATH.".to_string());
		}

		let version_str = String::from_utf8_lossy(&output.stdout);
		if !version_str.contains("nightly") {
			return Err(format!(
				"ripdoc requires a nightly Rust toolchain, but found: {}\nEnsure nightly Rust is installed and available in PATH.",
				version_str.trim()
			));
		}
	}

	Ok(())
}

/// Build a Ripdoc instance configured with common CLI knobs.
fn build_ripdoc(common: &CommonArgs) -> Ripdoc {
	Ripdoc::new()
		.with_offline(common.offline)
		.with_auto_impls(common.auto_impls)
		.with_render_format(common.format.into())
		.with_silent(!common.verbose)
		.with_source_labels(!common.no_source_labels)
}

/// Resolve the active search domains specified by the CLI flags.
fn search_domains_from_filters(filters: &SearchFilterArgs) -> SearchDomain {
	if filters.search_spec.is_empty() {
		SearchDomain::default()
	} else {
		filters
			.search_spec
			.iter()
			.fold(SearchDomain::empty(), |mut acc, spec| {
				acc |= SearchDomain::from(*spec);
				acc
			})
	}
}

/// Build a `SearchOptions` value using the provided CLI configuration and query.
fn build_search_options(
	common: &CommonArgs,
	filters: &SearchFilterArgs,
	query: &str,
) -> SearchOptions {
	let mut options = SearchOptions::new(query);
	options.include_private = common.private;
	options.case_sensitive = filters.search_case_sensitive;
	options.expand_containers = !filters.direct_match_only;
	options.domains = search_domains_from_filters(filters);
	options
}

/// Print a skeleton to stdout.
fn split_path_target_spec(value: &str) -> Option<(String, String)> {
	let split_at = value.find("::")?;
	let (left, right_with_sep) = value.split_at(split_at);
	let right = right_with_sep.strip_prefix("::")?;
	let left = left.trim();
	let right = right.trim();
	if left.is_empty() || right.is_empty() {
		return None;
	}

	let looks_like_path =
		left.contains('/') || left.contains('\\') || left.starts_with('.') || left.starts_with('/');
	if looks_like_path || std::path::Path::new(left).exists() {
		Some((left.to_string(), right.to_string()))
	} else {
		None
	}
}

#[derive(Debug, Clone)]
struct DiffHunk {
	file: std::path::PathBuf,
	start_line: usize,
	end_line: usize,
}

fn git_toplevel() -> Result<std::path::PathBuf, Box<dyn Error>> {
	let toplevel = ProcessCommand::new("git")
		.args(["rev-parse", "--show-toplevel"])
		.output()?;
	if !toplevel.status.success() {
		return Err("Failed to run `git rev-parse --show-toplevel`; are you in a git repo?".into());
	}
	let root = String::from_utf8_lossy(&toplevel.stdout);
	let root = root.trim();
	if root.is_empty() {
		return Err("`git rev-parse --show-toplevel` returned empty output".into());
	}
	Ok(std::path::PathBuf::from(root))
}

fn git_diff_text(rev_spec: Option<&str>, staged: bool) -> Result<String, Box<dyn Error>> {
	let mut cmd = ProcessCommand::new("git");
	cmd.args(["diff", "--unified=0", "--no-color"]);
	if staged {
		cmd.arg("--cached");
	}
	if let Some(spec) = rev_spec {
		cmd.arg(spec);
	}
	let output = cmd.output()?;
	if !output.status.success() {
		return Err("Failed to run `git diff --unified=0`".into());
	}
	Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Find a commit that touches Rust files by walking back from HEAD.
/// Returns the commit hash if found within the limit.
fn find_rust_touching_commit(limit: usize) -> Result<String, Box<dyn Error>> {
	// Get the last N commits
	let output = ProcessCommand::new("git")
		.args(["log", &format!("-{}", limit), "--format=%H"])
		.output()?;
	
	if !output.status.success() {
		return Err("Failed to run `git log`".into());
	}
	
	let commits = String::from_utf8_lossy(&output.stdout);
	for commit in commits.lines() {
		let commit = commit.trim();
		if commit.is_empty() {
			continue;
		}
		
		// Check if this commit touches any .rs files
		let files_output = ProcessCommand::new("git")
			.args(["diff-tree", "--no-commit-id", "--name-only", "-r", commit])
			.output()?;
		
		if files_output.status.success() {
			let files = String::from_utf8_lossy(&files_output.stdout);
			if files.lines().any(|f| f.trim().ends_with(".rs")) {
				return Ok(commit.to_string());
			}
		}
	}
	
	Err("No Rust-touching commit found".into())
}

fn parse_git_diff_hunks(diff: &str, git_root: &std::path::Path, only_rust: bool) -> Vec<DiffHunk> {
	let mut current_file: Option<std::path::PathBuf> = None;
	let mut hunks: Vec<DiffHunk> = Vec::new();

	fn parse_usize_prefix(s: &str) -> Option<(usize, &str)> {
		let mut end = 0usize;
		for (idx, ch) in s.char_indices() {
			if ch.is_ascii_digit() {
				end = idx + ch.len_utf8();
			} else {
				break;
			}
		}
		if end == 0 {
			return None;
		}
		let num = s[..end].parse::<usize>().ok()?;
		Some((num, &s[end..]))
	}

	for line in diff.lines() {
		if let Some(rest) = line.strip_prefix("+++ ") {
			let path = rest.trim();
			if path == "/dev/null" {
				current_file = None;
				continue;
			}
			let rel = path.strip_prefix("b/").unwrap_or(path);
			let abs = git_root.join(rel);
			if only_rust
				&& abs.extension().and_then(|e| e.to_str()) != Some("rs") {
					current_file = None;
					continue;
				}
			current_file = Some(abs);
			continue;
		}

		if !line.starts_with("@@") {
			continue;
		}
		let Some(ref file) = current_file else {
			continue;
		};

		let plus_idx = line
			.find(" +")
			.map(|i| i + 2)
			.or_else(|| line.find('+').map(|i| i + 1));
		let Some(plus_idx) = plus_idx else {
			continue;
		};
		let after_plus = &line[plus_idx..];
		let Some((start, rest)) = parse_usize_prefix(after_plus) else {
			continue;
		};
		let (len, _rest) = if let Some(rest) = rest.strip_prefix(',') {
			parse_usize_prefix(rest).unwrap_or((1, rest))
		} else {
			(1, rest)
		};

		let len = len.max(1);
		let end = start.saturating_add(len - 1).max(start);
		hunks.push(DiffHunk {
			file: file.clone(),
			start_line: start.max(1),
			end_line: end.max(1),
		});
	}

	// Preserve first-seen order but drop duplicates.
	let mut seen = std::collections::BTreeSet::new();
	hunks.retain(|h| seen.insert((h.file.clone(), h.start_line, h.end_line)));
	hunks
}

fn find_package_root(
	file: &std::path::Path,
	git_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
	let mut cur = file.parent()?.to_path_buf();
	loop {
		if cur.join("Cargo.toml").exists() {
			return Some(cur);
		}
		if cur == git_root {
			return None;
		}
		if !cur.pop() {
			return None;
		}
	}
}

fn resolve_changed_context(
	hunks: &[DiffHunk],
	rs: &Ripdoc,
	common: &CommonArgs,
) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
	const CONTEXT_LINES: usize = 30;
	const MAX_SNIPPET_LINES: usize = 220;
	const MAX_ITEMS_PER_HUNK: usize = 6;
	const NEAREST_ITEM_LIMIT: usize = 3;
	const NEAREST_ITEM_MAX_DISTANCE: usize = 80;
	const MAX_TARGETS: usize = 200;

	let git_root = git_toplevel()?;

	let mut targets: Vec<String> = Vec::new();
	let mut raw_specs: Vec<String> = Vec::new();
	let mut seen_targets = std::collections::BTreeSet::new();
	let mut seen_raw = std::collections::BTreeSet::new();

	let mut hunks_by_pkg: std::collections::HashMap<std::path::PathBuf, Vec<&DiffHunk>> =
		std::collections::HashMap::new();
	for hunk in hunks {
		let Some(pkg_root) = find_package_root(&hunk.file, &git_root) else {
			continue;
		};
		hunks_by_pkg.entry(pkg_root).or_default().push(hunk);
	}

	for (pkg_root, pkg_hunks) in hunks_by_pkg {
		let pkg_root_str = pkg_root.display().to_string();
		let resolved = resolve_target(&pkg_root_str, rs.offline());
		let Ok(resolved) = resolved else {
			continue;
		};

		for rt in resolved {
			let crate_data = match rt.read_crate(
				common.no_default_features,
				common.all_features,
				common.features.clone(),
				true,
				rs.silent(),
				rs.cache_config(),
			) {
				Ok(data) => data,
				Err(_) => continue,
			};

			let index = SearchIndex::build(&crate_data, true, Some(&pkg_root));

			let resolve_span_path = |span: &rustdoc_types::Span| -> std::path::PathBuf {
				let mut path = span.filename.clone();
				if path.is_relative() {
					let joined = pkg_root.join(&path);
					if joined.exists() {
						path = joined;
					} else {
						let mut components = span.filename.components();
						while components.next().is_some() {
							let candidate = pkg_root.join(components.as_path());
							if candidate.exists() {
								path = candidate;
								break;
							}
						}
					}
				}
				path.canonicalize().unwrap_or(path)
			};

			let mut entries_by_file: std::collections::HashMap<
				std::path::PathBuf,
				Vec<&ripdoc::core_api::search::SearchResult>,
			> = std::collections::HashMap::new();
			for entry in index.entries() {
				let Some(item) = crate_data.index.get(&entry.item_id) else {
					continue;
				};
				let Some(span) = &item.span else {
					continue;
				};
				let span_path = resolve_span_path(span);
				entries_by_file.entry(span_path).or_default().push(entry);
			}

			for hunk in &pkg_hunks {
				let file = hunk
					.file
					.canonicalize()
					.unwrap_or_else(|_| hunk.file.clone());
				let Some(entries) = entries_by_file.get(&file) else {
					continue;
				};

				let range_start = hunk.start_line.saturating_sub(CONTEXT_LINES).max(1);
				let range_end = hunk.end_line.saturating_add(CONTEXT_LINES).max(range_start);

				let mut candidates: Vec<(usize, usize, String)> = Vec::new();
				for entry in entries {
					let Some(item) = crate_data.index.get(&entry.item_id) else {
						continue;
					};
					let Some(span) = &item.span else {
						continue;
					};
					let begin = span.begin.0;
					let end = span.end.0;
					if begin == 0 || end == 0 {
						continue;
					}

					let overlaps = begin <= range_end && end >= range_start;
					let distance = if overlaps {
						0
					} else if end < range_start {
						range_start - end
					} else { begin.saturating_sub(range_end) };

					let kind_priority = match entry.kind {
						SearchItemKind::Method
						| SearchItemKind::Function
						| SearchItemKind::Struct
						| SearchItemKind::Enum
						| SearchItemKind::Trait
						| SearchItemKind::TypeAlias => 0usize,
						SearchItemKind::Module => 2usize,
						_ => 3usize,
					};

					let spec = format!("{}::{}", pkg_root.display(), entry.path_string);
					candidates.push((distance, kind_priority, spec));
				}

				candidates.sort_by_key(|(dist, pri, spec)| (*dist, *pri, spec.len()));

				let mut added_for_hunk = 0usize;
				for (dist, _pri, spec) in &candidates {
					if *dist != 0 {
						continue;
					}
					if targets.len() >= MAX_TARGETS {
						break;
					}
					if seen_targets.insert(spec.clone()) {
						targets.push(spec.clone());
						added_for_hunk += 1;
						if added_for_hunk >= MAX_ITEMS_PER_HUNK {
							break;
						}
					}
				}

				if added_for_hunk == 0 {
					let mut nearest_added = 0usize;
					for (dist, _pri, spec) in &candidates {
						if *dist == 0 || *dist > NEAREST_ITEM_MAX_DISTANCE {
							continue;
						}
						if targets.len() >= MAX_TARGETS {
							break;
						}
						if seen_targets.insert(spec.clone()) {
							targets.push(spec.clone());
							nearest_added += 1;
							if nearest_added >= NEAREST_ITEM_LIMIT {
								break;
							}
						}
					}
				}

				let snippet_start = range_start;
				let mut snippet_end = range_end;
				let max_end = snippet_start.saturating_add(MAX_SNIPPET_LINES.saturating_sub(1));
				if snippet_end > max_end {
					snippet_end = max_end;
				}
				let spec = format!("{}:{}:{}", file.display(), snippet_start, snippet_end);
				if seen_raw.insert(spec.clone()) {
					raw_specs.push(spec);
				}
			}
		}
	}

	Ok((targets, raw_specs))
}

#[cfg(test)]
mod diff_tests {
	use super::{DiffHunk, parse_git_diff_hunks};

	#[test]
	fn parse_git_diff_hunks_extracts_new_ranges() {
		let diff = "diff --git a/src/lib.rs b/src/lib.rs\nindex 111..222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +10,3 @@\n+added\n";
		let root = std::path::PathBuf::from("/repo");
		let hunks = parse_git_diff_hunks(diff, &root, true);
		assert_eq!(hunks.len(), 1);
		let DiffHunk {
			file,
			start_line,
			end_line,
		} = &hunks[0];
		assert!(file.ends_with("src/lib.rs"));
		assert_eq!((*start_line, *end_line), (10, 12));
	}
}

/// Print a skeleton to stdout.
fn run_print(common: &CommonArgs, args: &PrintArgs, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	let mut target = args.target.clone();
	let mut item_query = args.item.clone();

	if args.search.is_none() && item_query.is_none()
		&& let Some((split_target, split_query)) = split_path_target_spec(&args.target) {
			target = split_target;
			item_query = Some(split_query);
		}

	let explicit_search = args.search.as_deref();
	let implicit_search = item_query.as_deref();
	let query = explicit_search.or(implicit_search);

	// If search query is provided, use search mode.
	if let Some(query) = query {
		let trimmed = query.trim();
		if trimmed.is_empty() {
			println!("Search query is empty; nothing to do.");
			return Ok(());
		}

		let mut options = build_search_options(common, &args.filters, trimmed);
		if args.search.is_none() {
			// Positional item mode: always treat as a path query.
			options.domains = SearchDomain::PATHS;
		}

		let response = rs.search(
			&target,
			common.no_default_features,
			common.all_features,
			common.features.clone(),
			&options,
			args.implementation,
			args.raw_source,
		)?;

		if response.results.is_empty() && response.rendered.is_empty() {
			println!("No matches found for \"{}\".", trimmed);
			if trimmed.contains("::") {
				let last_segment = trimmed.rsplit("::").next().unwrap_or(trimmed);
				println!(
					"Tip: discover the exact rustdoc path with: ripdoc list {} --search \"{}\" --search-spec path --private",
					target, last_segment
				);
				println!(
					"Tip: if the code isn't in rustdoc output, use `ripdoc skelebuild add-file <path>` or `ripdoc skelebuild add-raw <path[:start[:end]]>` to include raw source."
				);
			}
			return Ok(());
		}

		let output = if should_color_output(common) {
			highlight_matches(
				&response.rendered,
				trimmed,
				args.filters.search_case_sensitive,
			)
		} else {
			response.rendered
		};

		print!("{}", output);
		return Ok(());
	}

	// Normal print mode
	let output = rs.render(
		&target,
		common.no_default_features,
		common.all_features,
		common.features.clone(),
		common.private,
		args.implementation,
		args.raw_source,
	)?;

	println!("{output}");

	Ok(())
}

/// Output raw rustdoc JSON.
fn run_raw(common: &CommonArgs, target: &str, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	let output = rs.raw_json(
		target,
		common.no_default_features,
		common.all_features,
		common.features.clone(),
		common.private,
	)?;

	println!("{output}");

	Ok(())
}

/// Execute the list flow and print a structured item summary.
fn run_list(common: &CommonArgs, args: &ListArgs, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	let mut search_options: Option<SearchOptions> = None;
	let mut trimmed_query: Option<String> = None;

	if let Some(query) = args.search.as_deref() {
		let trimmed = query.trim();
		if trimmed.is_empty() {
			println!("Search query is empty; nothing to do.");
			return Ok(());
		}
		trimmed_query = Some(trimmed.to_string());
		let mut options = build_search_options(common, &args.filters, trimmed);
		// Heuristic: queries that look like `crate::module::Item` are usually path searches.
		if trimmed.contains("::") && !options.domains.contains(SearchDomain::PATHS) {
			options.domains |= SearchDomain::PATHS;
		}
		search_options = Some(options);
	}

	let listings = rs.list(
		&args.target,
		common.no_default_features,
		common.all_features,
		common.features.clone(),
		common.private,
		search_options.as_ref(),
	)?;

	if listings.is_empty() {
		if let Some(query) = trimmed_query {
			println!("No matches found for \"{query}\".");
			if !common.private {
				println!("Tip: pass `--private` to include private items.");
			}
			if query.contains("::")
				&& !args
					.filters
					.search_spec
					.iter()
					.any(|spec| matches!(spec, SearchSpec::Path))
			{
				println!("Tip: pass `--search-spec path` to search canonical item paths.");
			}
		} else {
			println!("No items found.");
			if !common.private {
				println!("Tip: pass `--private` to include private items.");
			}
		}
		return Ok(());
	}

	// Use JSON format if requested
	if common.format == OutputFormat::Json {
		use ripdoc::build_list_tree;
		let tree = build_list_tree(&listings);
		let json = serde_json::to_string_pretty(&tree)?;
		println!("{json}");
		return Ok(());
	}

	let label_width = listings
		.iter()
		.map(|entry| entry.kind.label().len())
		.max()
		.unwrap_or(0);
	let path_width = listings
		.iter()
		.map(|entry| entry.path.len())
		.max()
		.unwrap_or(0);

	let mut buffer = String::new();
	for entry in listings {
		let label = entry.kind.label();
		let location = format_source_location(entry.source.as_ref());
		let line = format!(
			"{label:<label_width$} {path:<path_width$} {location}\n",
			path = entry.path
		);
		let highlighted_line = if let Some(ref query) = trimmed_query {
			if should_color_output(common) {
				highlight_matches(&line, query, args.filters.search_case_sensitive)
			} else {
				line
			}
		} else {
			line
		};

		buffer.push_str(&highlighted_line);
	}

	print!("{}", buffer);

	Ok(())
}

/// Format a source location for display.
fn format_source_location(source: Option<&SourceLocation>) -> String {
	match source {
		Some(location) => {
			let mut rendered = location.path.clone();
			if let Some(line) = location.line {
				rendered.push_str(&format!(":{line}"));
			}
			rendered
		}
		None => "-".to_string(),
	}
}

/// Fetch and print the README for the target crate.
fn run_readme(common: &CommonArgs, args: &ReadmeArgs) -> Result<(), Box<dyn Error>> {
	use std::env;
	use std::path::PathBuf;

	use ripdoc::cargo_utils::target::{Entrypoint, Target};

	// Parse the target first to understand what type it is
	let target_parsed = Target::parse(&args.target)?;

	// Determine the starting path for local README search
	let search_path: Option<PathBuf> = match &target_parsed.entrypoint {
		Entrypoint::Path(path) => Some(if path.is_absolute() {
			path.clone()
		} else {
			env::current_dir()?.join(path)
		}),
		Entrypoint::Name { name: _, .. } => {
			// Try to resolve target to see if it's a local workspace member or dependency
			resolve_target(&args.target, common.offline)
				.ok()
				.and_then(|resolved_list| {
					resolved_list
						.first()
						.map(|resolved| resolved.package_root().to_path_buf())
				})
		}
	};

	// If we have a local path to search, look for README there and in parent directories
	if let Some(mut current_path) = search_path {
		if let Ok(canonical) = current_path.canonicalize() {
			current_path = canonical;
		}

		// Try current directory and up to 5 parent directories
		let cargo_path = ripdoc::cargo_utils::CargoPath::Path(current_path.clone());
		if let Ok(Some(content)) = cargo_path.find_readme() {
			println!("{}", content);
			return Ok(());
		}
		let mut parent_path = current_path.parent();
		let mut depth = 0;
		while let Some(parent) = parent_path {
			if depth >= 5 {
				break;
			}
			let parent_cargo_path = ripdoc::cargo_utils::CargoPath::Path(parent.to_path_buf());
			if let Ok(Some(content)) = parent_cargo_path.find_readme() {
				println!("{}", content);
				return Ok(());
			}
			parent_path = parent.parent();
			depth += 1;
		}
	}

	// Try fetching from crates.io
	match target_parsed.entrypoint {
		Entrypoint::Name { name, version } => {
			if common.offline {
				// Try to find the latest cached version
				if let Some((crate_path, found_version)) = find_latest_cached_version(&name)? {
					let cargo_path = ripdoc::cargo_utils::CargoPath::Path(crate_path);
					if let Ok(Some(content)) = cargo_path.find_readme() {
						eprintln!(
							"Using cached version {} (latest available locally)",
							found_version
						);
						println!("{}", content);
						return Ok(());
					}
				}

				return Err(format!(
					"README not found locally for '{}'. \
					 When using --offline, either:\n\
					 1. Specify a version (e.g., 'ripdoc readme {}@version')\n\
					 2. Run without --offline to fetch from crates.io",
					name, name
				)
				.into());
			}
			let readme = fetch_readme(&name, version.as_ref())?;
			println!("{}", readme);
			Ok(())
		}
		_ => Err("README not found for this target".into()),
	}
}

fn should_color_output(common: &CommonArgs) -> bool {
	if common.no_color {
		return false;
	}
	if std::env::var_os("NO_COLOR").is_some() {
		return false;
	}
	if std::env::var("TERM").ok().as_deref() == Some("dumb") {
		return false;
	}
	std::io::stdout().is_terminal()
}

/// Highlight all occurrences of the search query in the given text.
///
/// Queries containing pipe characters are treated as OR patterns and use regex highlighting.
/// Single-term queries use substring-based highlighting for better performance.
///
/// Matches are highlighted in bright green and bold using ANSI escape codes.
fn highlight_matches(text: &str, query: &str, case_sensitive: bool) -> String {
	if query.is_empty() {
		return text.to_string();
	}

	if query.contains('|') {
		highlight_matches_regex(text, query, case_sensitive)
	} else {
		highlight_matches_simple(text, query, case_sensitive)
	}
}

/// Highlight matches using substring search for single-term queries.
///
/// This performs simple string containment matching and highlights all occurrences.
/// More efficient than regex for single-term searches.
fn highlight_matches_simple(text: &str, query: &str, case_sensitive: bool) -> String {
	let mut result = String::with_capacity(text.len() * 2);
	let search_text = if case_sensitive {
		text.to_string()
	} else {
		text.to_lowercase()
	};
	let search_query = if case_sensitive {
		query.to_string()
	} else {
		query.to_lowercase()
	};

	let mut last_end = 0;
	let mut search_start = 0;

	while let Some(pos) = search_text[search_start..].find(&search_query) {
		let absolute_pos = search_start + pos;
		result.push_str(&text[last_end..absolute_pos]);
		let match_end = absolute_pos + query.len();
		let matched_text = &text[absolute_pos..match_end];
		result.push_str(&matched_text.to_string().bright_green().bold().to_string());
		last_end = match_end;
		search_start = match_end;
	}

	result.push_str(&text[last_end..]);
	result
}

/// Highlight matches using regex for OR queries containing pipe characters.
///
/// The pipe character is treated as a regex OR operator while other regex
/// metacharacters are escaped. Falls back to substring highlighting if regex
/// compilation fails.
fn highlight_matches_regex(text: &str, pattern: &str, case_sensitive: bool) -> String {
	let escaped_pattern = escape_regex_preserving_pipes(pattern);

	let regex = match if case_sensitive {
		Regex::new(&escaped_pattern)
	} else {
		Regex::new(&format!("(?i){}", escaped_pattern))
	} {
		Ok(re) => re,
		Err(_) => {
			return highlight_matches_simple(text, pattern, case_sensitive);
		}
	};

	let mut result = String::with_capacity(text.len() * 2);
	let mut last_end = 0;

	for mat in regex.find_iter(text) {
		result.push_str(&text[last_end..mat.start()]);
		let matched_text = &text[mat.start()..mat.end()];
		result.push_str(&matched_text.to_string().bright_green().bold().to_string());
		last_end = mat.end();
	}

	result.push_str(&text[last_end..]);
	result
}

fn main() {
	let cli = Cli::parse();
	if let Err(e) = check_nightly_toolchain() {
		eprintln!("{e}");
		process::exit(1);
	}

	let result = run(cli);

	if let Err(e) = result {
		eprintln!("{e}");
		process::exit(1);
	}
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
	match cli.command {
		Command::Print(args) => {
			let rs = build_ripdoc(&args.common);
			run_print(&args.common, &args, &rs)
		}
		Command::Raw(args) => {
			if args.item.is_some()
				|| args.search.is_some()
				|| args.implementation
				|| args.raw_source
			{
				return Err(
					"`ripdoc raw` only accepts a target (no item/search/source flags).".into(),
				);
			}
			let rs = build_ripdoc(&args.common);
			run_raw(&args.common, &args.target, &rs)
		}
		Command::List(args) => {
			let rs = build_ripdoc(&args.common);
			run_list(&args.common, &args, &rs)
		}
		Command::Readme(args) => run_readme(&args.common, &args),
		Command::Agents(args) => {
			match args.topic {
				None => print!("{}", include_str!("agents_ripdoc.md")),
				Some(AgentsTopic::Skelebuild) => {
					print!("{}", include_str!("skelebuild/agents_skelebuild.md"))
				}
			}
			Ok(())
		}
		Command::Skelebuild(args) => {
			use ripdoc::skelebuild::SkeleAction;
			let rs = build_ripdoc(&args.common);

			let mut output = args.output;
			let mut plain: Option<bool> = if args.plain {
				Some(true)
			} else if args.no_plain {
				Some(false)
			} else {
				None
			};

			let action = if args.reset {
				Some(SkeleAction::Reset)
			} else if let Some(cmd) = args.command {
				match cmd {
		SkelebuildSubcommand::Add {
			target,
			items,
			implementation,
			no_implementation,
			raw_source,
			private,
			no_private,
			no_validate,
			strict,
			output: o,
			plain: p,
		} => {
			if o.is_some() {
				output = o;
			}
			if p {
				plain = Some(true);
			}

			let validate = !no_validate;
			let effective_private = private && !no_private;
			let effective_implementation = implementation && !no_implementation;
			let target_prefix = target.clone();
			let targets: Vec<String> = if items.is_empty() {
				vec![target]
			} else {
				items
					.into_iter()
					.map(|item| format!("{target_prefix}::{item}"))
					.collect()
			};

			if targets.len() == 1 {
				Some(SkeleAction::Add {
					target: targets[0].clone(),
					implementation: effective_implementation,
					raw_source,
					validate,
					private: effective_private,
					strict,
				})
			} else {
				Some(SkeleAction::AddMany {
					targets,
					implementation: effective_implementation,
					raw_source,
					validate,
					private: effective_private,
					strict,
				})
			}
		}

					SkelebuildSubcommand::AddRaw { spec, output: o } => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::AddRaw { spec })
					}
					SkelebuildSubcommand::AddFile { file, output: o } => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::AddRaw {
							spec: file.display().to_string(),
						})
					}
				SkelebuildSubcommand::AddChanged {
					git,
					staged,
					only_rust,
					output: o,
				} => {
					if o.is_some() {
						output = o;
					}
					let git_root = git_toplevel()?;
					let revspec = git.as_deref().unwrap_or(if staged { "--cached" } else { "HEAD" });
					
					eprintln!("Analyzing changes (revspec: {})...", revspec);
					
					let diff = git_diff_text(git.as_deref(), staged)?;
					let all_hunks = parse_git_diff_hunks(&diff, &git_root, false);
					let filtered_hunks = if only_rust {
						parse_git_diff_hunks(&diff, &git_root, true)
					} else {
						all_hunks.clone()
					};
					
					// Count unique changed files
					let mut all_files = std::collections::BTreeSet::new();
					let mut filtered_files = std::collections::BTreeSet::new();
					for hunk in &all_hunks {
						all_files.insert(hunk.file.clone());
					}
					for hunk in &filtered_hunks {
						filtered_files.insert(hunk.file.clone());
					}
					
					if filtered_hunks.is_empty() {
						// Print structured empty report
						eprintln!("\nNo changed hunks found.");
						eprintln!("\nDiagnostics:");
						eprintln!("  Resolved revspec: {}", revspec);
						eprintln!("  Total changed files discovered: {}", all_files.len());
						eprintln!("  Total hunks discovered (before filtering): {}", all_hunks.len());
						
						if only_rust {
							let files_filtered = all_files.len() - filtered_files.len();
							let hunks_filtered = all_hunks.len() - filtered_hunks.len();
							eprintln!("  Files filtered out by --only-rust: {}", files_filtered);
							eprintln!("  Hunks filtered out by --only-rust: {}", hunks_filtered);
							
							if hunks_filtered > 0 {
								eprintln!("\nAll changes were filtered out by `--only-rust`.");
								eprintln!("\nExcluded files (first 20):");
								let non_rust_files: Vec<_> = all_files.difference(&filtered_files).collect();
								for (i, file) in non_rust_files.iter().take(20).enumerate() {
									eprintln!("  {}. {}", i + 1, file.display());
								}
								if non_rust_files.len() > 20 {
									eprintln!("  ... and {} more", non_rust_files.len() - 20);
								}
								eprintln!("\nSuggestions:");
								eprintln!("  - Try removing --only-rust to include all changed files");
								eprintln!("  - Try expanding the range (e.g., HEAD~2..HEAD or main..HEAD)");
							}
						} else {
							eprintln!("\nSuggestions:");
							eprintln!("  - Verify the revspec is correct: {}", revspec);
							eprintln!("  - Try expanding the range (e.g., HEAD~2..HEAD or main..HEAD)");
							if !staged {
								eprintln!("  - Or use --staged to check staged changes");
							}
						}
						
						// Optional: compute a concrete suggestion by walking back
						if only_rust {
							eprintln!("\nSearching for recent Rust-touching commits...");
							if let Ok(suggestion) = find_rust_touching_commit(50) {
								eprintln!("  Found commit: {}", suggestion);
								eprintln!("  Try: ripdoc skelebuild add-changed --git {}..HEAD --only-rust", suggestion);
							} else {
								eprintln!("  No Rust-touching commit found in last 50 commits.");
							}
						}
						
						return Ok(());
					}
					let (targets, raw_specs) =
						resolve_changed_context(&filtered_hunks, &rs, &args.common)?;
					if targets.is_empty() && raw_specs.is_empty() {
						eprintln!("No changed context could be resolved.");
						eprintln!("\nDiagnostics:");
						eprintln!("  Hunks found: {}", filtered_hunks.len());
						eprintln!("  Files changed: {}", filtered_files.len());
						eprintln!("\nNote: Hunks were found but couldn't be resolved to rustdoc targets.");
						eprintln!("      This may happen if changes are in files without rustdoc coverage.");
						return Ok(());
					}
					Some(SkeleAction::AddChangedResolved { targets, raw_specs })
				}
					SkelebuildSubcommand::Update {
						spec,
						implementation,
						no_implementation,
						raw_source,
						no_raw_source,
						output: o,
					} => {
						if o.is_some() {
							output = o;
						}
						let impl_value = if implementation {
							Some(true)
						} else if no_implementation {
							Some(false)
						} else {
							None
						};
						let raw_value = if raw_source {
							Some(true)
						} else if no_raw_source {
							Some(false)
						} else {
							None
						};
						Some(SkeleAction::Update {
							spec,
							implementation: impl_value,
							raw_source: raw_value,
						})
					}
				SkelebuildSubcommand::Inject {
					content,
					from_stdin,
					from_file,
					literal,
					after,
					after_target,
					before_target,
					at,
					output: o,
				} => {
					if o.is_some() {
						output = o;
					}

					use std::io::{IsTerminal, Read};
					
					let content = if from_stdin {
						// Explicit --from-stdin flag
						let mut buf = String::new();
						std::io::stdin().read_to_string(&mut buf)?;
						buf
					} else if let Some(path) = from_file {
						// Read from file
						std::fs::read_to_string(path)?
					} else if let Some(c) = content {
						// Positional content provided
						c
					} else {
						// No content, no --from-stdin, no --from-file
						// Auto-detect: if stdin is not a TTY, read from it
						if !std::io::stdin().is_terminal() {
							let mut buf = String::new();
							std::io::stdin().read_to_string(&mut buf)?;
							buf
						} else {
							// stdin is a TTY and no content provided
							return Err(
								"Missing required argument: <CONTENT>\n\n\
								The `inject` command requires content to inject. You can provide it in one of these ways:\n\n\
								  1. As a positional argument:\n\
								     ripdoc skelebuild inject \"your content here\" --at 0\n\n\
								  2. Via stdin with a heredoc:\n\
								     ripdoc skelebuild inject --at 0 <<'EOF'\n\
								     your content here\n\
								     EOF\n\n\
								  3. Via stdin with a pipe:\n\
								     cat file | ripdoc skelebuild inject --at 0\n\n\
								  4. Explicitly from stdin:\n\
								     ripdoc skelebuild inject --from-stdin --at 0 <<'EOF'\n\
								     your content here\n\
								     EOF\n\n\
								  5. From a file:\n\
								     ripdoc skelebuild inject --from-file path/to/file.txt --at 0"
									.into(),
							);
						}
					};

					Some(SkeleAction::Inject {
						content,
						literal,
						after,
						after_target,
						before_target,
						at,
					})
				}

					SkelebuildSubcommand::Remove { target, output: o } => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::Remove(target))
					}
					SkelebuildSubcommand::Reset {
						output: o,
						plain: p,
					} => {
						if o.is_some() {
							output = o;
						}
						if p {
							plain = Some(true);
						}
						Some(SkeleAction::Reset)
					}
					SkelebuildSubcommand::Status { keys } => Some(SkeleAction::Status { keys }),
					SkelebuildSubcommand::Preview => Some(SkeleAction::Preview),
					SkelebuildSubcommand::Rebuild => Some(SkeleAction::Rebuild),
				}
			} else {
				None
			};

			ripdoc::skelebuild::run_skelebuild(action, output, plain, args.show_state, &rs)?;
			Ok(())
		}
	}
}
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
/// Output formats the CLI can emit.
enum OutputFormat {
	/// Print formatted Rust code.
	#[value(alias = "rs")]
	Rust,
	/// Print Markdown with stripped documentation markers (default).
	#[value(alias = "md")]
	Markdown,
	/// Print JSON output (only for list command).
	Json,
}

impl From<OutputFormat> for RenderFormat {
	fn from(format: OutputFormat) -> Self {
		match format {
			OutputFormat::Rust => RenderFormat::Rust,
			OutputFormat::Markdown => RenderFormat::Markdown,
			// JSON format doesn't have a RenderFormat equivalent; it's only for list output
			OutputFormat::Json => RenderFormat::Markdown,
		}
	}
}
