//! CLI entrypoint.

use std::error::Error;
use std::process::{self, Command as ProcessCommand, Stdio};

use clap::{Args, Parser, Subcommand, ValueEnum};
use owo_colors::OwoColorize;
use ripdoc_core::{RenderFormat, Ripdoc, SearchDomain, SearchOptions, SourceLocation};

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
	/// Render auto-implemented traits
	#[arg(short = 'i', long, default_value_t = false)]
	auto_impls: bool,

	/// Render private items
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

	/// Enable verbose mode, showing cargo output while rendering docs
	#[arg(short = 'v', long, default_value_t = false)]
	verbose: bool,

	/// Select the render format (`rust` or `markdown`)
	#[arg(short = 'f', long, value_enum, default_value = "markdown")]
	format: OutputFormat,
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

	/// Optional search query used to filter the listing.
	#[arg(short = 's', long)]
	query: Option<String>,

	#[command(flatten)]
	filters: SearchFilterArgs,
}

#[derive(Args, Clone)]
struct SearchArgs {
	/// Target to generate - a directory, file path, or a module name
	target: String,

	/// Search query used to filter the generated skeleton instead of rendering everything.
	#[arg(required = false)]
	query: Option<String>,

	#[command(flatten)]
	filters: SearchFilterArgs,
}

#[derive(Args, Clone)]
struct RenderArgs {
	/// Target to generate - a directory, file path, or a module name
	#[arg(default_value = "./")]
	target: String,
}

#[derive(Subcommand, Clone)]
enum Command {
	/// Render a crate skeleton (default).
	Render(RenderArgs),
	/// Produce a structured item listing.
	List(ListArgs),
	/// Search for matching items and render the filtered skeleton.
	Search(SearchArgs),
	/// Emit raw rustdoc JSON.
	Raw(RenderArgs),
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
/// Parsed command-line options for the ripdoc CLI.
struct Cli {
	#[command(flatten)]
	common: CommonArgs,

	#[arg()]
	legacy_target: Option<String>,

	#[arg(trailing_var_arg = true, hide = true)]
	legacy_extra: Vec<String>,

	#[command(subcommand)]
	command: Option<Command>,
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

/// Render a skeleton locally and stream it to stdout or a pager.
fn run_render(common: &CommonArgs, target: &str, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	let output = rs.render(
		target,
		common.no_default_features,
		common.all_features,
		common.features.clone(),
		common.private,
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

	if let Some(query) = args.query.as_deref() {
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
		buffer.push_str(&format!(
			"{label:<label_width$} {path:<path_width$} {location}\n",
			path = entry.path
		));
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

/// Highlight all occurrences of the search query.
fn highlight_matches(text: &str, query: &str, case_sensitive: bool) -> String {
	if query.is_empty() {
		return text.to_string();
	}

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
		// Add text before the match
		result.push_str(&text[last_end..absolute_pos]);
		// Add the highlighted match
		let match_end = absolute_pos + query.len();
		let matched_text = &text[absolute_pos..match_end];
		result.push_str(&matched_text.to_string().bright_green().bold().to_string());
		last_end = match_end;
		search_start = match_end;
	}

	// Add remaining text
	result.push_str(&text[last_end..]);
	result
}

/// Execute the search flow and print the filtered skeleton to stdout.
fn run_search(common: &CommonArgs, args: &SearchArgs, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	if args.query.is_none() {
		return run_cargo_search_fallback(&args.target, common.offline);
	}
	let trimmed = args.query.as_deref().unwrap().trim();
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

	Ok(())
}

/// Fallback to `cargo search` when a query is missing.
fn run_cargo_search_fallback(term: &str, offline: bool) -> Result<(), Box<dyn Error>> {
	if offline {
		return Err("--offline cannot be used with cargo search fallback. Please provide a query or re-run without --offline.".into());
	}

	let status = ProcessCommand::new("cargo")
		.arg("search")
		.arg(term)
		.status()
		.map_err(|e| format!("Failed to invoke cargo search: {e}"))?;

	if !status.success() {
		return Err(format!(
			"`cargo search {term}` failed with exit code {}",
			status.code().unwrap_or(-1)
		)
		.into());
	}

	Ok(())
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
	let common = cli.common;
	let rs = build_ripdoc(&common);

	match cli.command {
		Some(Command::Render(args)) => run_render(&common, &args.target, &rs),
		Some(Command::Raw(args)) => run_raw(&common, &args.target, &rs),
		Some(Command::List(args)) => run_list(&common, &args, &rs),
		Some(Command::Search(args)) => run_search(&common, &args, &rs),
		None => {
			let default_target = cli.legacy_target.unwrap_or_else(|| "./".to_string());
			if !cli.legacy_extra.is_empty() {
				let mut extras = cli.legacy_extra;
				if extras.first().is_some_and(|s| s == "search") {
					extras.remove(0);
				}
				let query = extras.join(" ").trim().to_string();
				if query.is_empty() {
					return Err("A search query is required when trailing arguments are provided without a subcommand.".into());
				}
				let search_args = SearchArgs {
					target: default_target,
					query: Some(query),
					filters: SearchFilterArgs::default(),
				};
				run_search(&common, &search_args, &rs)
			} else {
				run_render(&common, &default_target, &rs)
			}
		}
	}
}
#[derive(Debug, Clone, Copy, ValueEnum)]
/// Output formats the CLI can emit.
enum OutputFormat {
	/// Render formatted Rust code (default).
	#[value(alias = "rs")]
	Rust,
	/// Emit Markdown with stripped documentation markers.
	#[value(alias = "md")]
	Markdown,
}

impl From<OutputFormat> for RenderFormat {
	fn from(format: OutputFormat) -> Self {
		match format {
			OutputFormat::Rust => RenderFormat::Rust,
			OutputFormat::Markdown => RenderFormat::Markdown,
		}
	}
}
