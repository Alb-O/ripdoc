//! CLI entrypoint.

use std::error::Error;
use std::process::{self, Command as ProcessCommand, Stdio};

use clap::{Args, Parser, Subcommand, ValueEnum};
use owo_colors::OwoColorize;
use regex::Regex;
use ripdoc::cargo_utils::{fetch_readme, find_latest_cached_version, resolve_target};
use ripdoc::core_api::pattern::escape_regex_preserving_pipes;
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

	/// Search query used to filter the printed skeleton
	#[arg(short = 's', long)]
	search: Option<String>,

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

#[derive(Args, Clone)]
/// Arguments for the `skelebuild` subcommand.
struct SkelebuildArgs {
	#[command(subcommand)]
	command: Option<SkelebuildSubcommand>,

	/// Target to add (shorthand for 'add').
	target: Option<String>,

	/// Output file for the skeleton.
	#[arg(short = 'O', long)]
	output: Option<std::path::PathBuf>,

	/// Reset the current skelebuild state.
	#[arg(long)]
	reset: bool,

	/// Flatten the output (skip module nesting).
	#[arg(long)]
	flat: bool,

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

		/// Include the full source code for this item.
		#[arg(short = 'f', long, default_value_t = false)]
		full: bool,

		/// Output file for the skeleton.
		#[arg(short = 'O', long)]
		output: Option<std::path::PathBuf>,

		/// Flatten the output.
		#[arg(long)]
		flat: bool,
	},
	/// Inject manual commentary.
	Inject {
		/// Text to inject.
		content: String,

		/// Inject after this target.
		#[arg(long)]
		after: Option<String>,

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
	},
	/// Show current targets and output path.
	Status,
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
fn run_print(common: &CommonArgs, args: &PrintArgs, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	// If search query is provided, use search mode
	if let Some(query) = &args.search {
		let trimmed = query.trim();
		if trimmed.is_empty() {
			println!("Search query is empty; nothing to do.");
			return Ok(());
		}

		let options = build_search_options(common, &args.filters, trimmed);

		let response = rs.search(
			&args.target,
			common.no_default_features,
			common.all_features,
			common.features.clone(),
			&options,
		)?;

		if response.results.is_empty() {
			println!("No matches found for \"{}\".", trimmed);
			return Ok(());
		}

		let output = highlight_matches(
			&response.rendered,
			trimmed,
			args.filters.search_case_sensitive,
		);

		print!("{}", output);
	} else {
		// Normal print mode
		let output = rs.render(
			&args.target,
			common.no_default_features,
			common.all_features,
			common.features.clone(),
			common.private,
		)?;

		println!("{output}");
	}

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
		search_options = Some(build_search_options(common, &args.filters, trimmed));
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
		} else {
			println!("No items found.");
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
			highlight_matches(&line, query, args.filters.search_case_sensitive)
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
			let rs = build_ripdoc(&args.common);
			run_raw(&args.common, &args.target, &rs)
		}
		Command::List(args) => {
			let rs = build_ripdoc(&args.common);
			run_list(&args.common, &args, &rs)
		}
		Command::Readme(args) => run_readme(&args.common, &args),
		Command::Skelebuild(args) => {
			use ripdoc::skelebuild::SkeleAction;
			let rs = build_ripdoc(&args.common);

			let mut output = args.output;
			let mut flat = args.flat;

			let action = if args.reset {
				Some(SkeleAction::Reset)
			} else if let Some(cmd) = args.command {
				match cmd {
					SkelebuildSubcommand::Add {
						target,
						full,
						output: o,
						flat: f,
					} => {
						if o.is_some() {
							output = o;
						}
						if f {
							flat = f;
						}
						Some(SkeleAction::Add { target, full })
					}
					SkelebuildSubcommand::Inject {
						content,
						after,
						output: o,
					} => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::Inject { content, after })
					}
					SkelebuildSubcommand::Remove { target, output: o } => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::Remove(target))
					}
					SkelebuildSubcommand::Reset { output: o } => {
						if o.is_some() {
							output = o;
						}
						Some(SkeleAction::Reset)
					}
					SkelebuildSubcommand::Status => Some(SkeleAction::Status),
				}
			} else if let Some(target) = args.target {
				Some(SkeleAction::Add {
					target,
					full: false,
				})
			} else {
				None
			};

			ripdoc::skelebuild::run_skelebuild(action, output, flat, &rs)?;
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
