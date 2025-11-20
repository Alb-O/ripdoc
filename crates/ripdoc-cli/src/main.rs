//! CLI entrypoint.

use std::error::Error;
use std::process::{self, Command, Stdio};

use clap::{Parser, ValueEnum};
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

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
/// Parsed command-line options for the ripdoc CLI.
struct Cli {
	/// Target to generate - a directory, file path, or a module name
	#[arg(default_value = "./")]
	target: String,

	/// Output raw JSON instead of rendered Rust code
	#[arg(short = 'r', long, default_value_t = false)]
	raw: bool,

	/// Search query used to filter the generated skeleton instead of rendering everything.
	#[arg(short = 's', long)]
	search: Option<String>,

	/// Output a structured item listing instead of rendered code.
	#[arg(short = 'l', long, default_value_t = false, conflicts_with = "raw")]
	list: bool,

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

	/// Disable caching of rustdoc JSON output
	#[arg(long, default_value_t = false)]
	no_cache: bool,

	/// Custom directory for storing cached rustdoc JSON output
	#[arg(long, value_name = "DIR")]
	cache_dir: Option<String>,

	/// Select the render format (`rust` or `markdown`)
	#[arg(short = 'f', long, value_enum, default_value = "markdown")]
	format: OutputFormat,
}

/// Ensure the nightly toolchain and rust-docs JSON component are present.
fn check_nightly_toolchain() -> Result<(), String> {
	// First, check if rustup is available
	let rustup_available = Command::new("rustup")
		.arg("--version")
		.stderr(Stdio::null())
		.stdout(Stdio::null())
		.status()
		.map(|status| status.success())
		.unwrap_or(false);

	if rustup_available {
		// Check if nightly toolchain is installed via rustup
		let output = Command::new("rustup")
			.args(["run", "nightly", "rustc", "--version"])
			.stderr(Stdio::null())
			.output()
			.map_err(|e| format!("Failed to run rustup: {e}"))?;

		if !output.status.success() {
			return Err("ripdoc requires the nightly toolchain to be installed.\nRun: rustup toolchain install nightly".to_string());
		}
	} else {
		// rustup is not available - check for nightly rustc directly
		let output = Command::new("rustc")
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

/// Render a skeleton locally and stream it to stdout or a pager.
fn run_cmdline(cli: &Cli) -> Result<(), Box<dyn Error>> {
	let mut rs = Ripdoc::new()
		.with_offline(cli.offline)
		.with_auto_impls(cli.auto_impls)
		.with_render_format(cli.format.into())
		.with_silent(!cli.verbose);

	// Configure caching
	if cli.no_cache {
		rs = rs.with_cache(false);
	}
	if let Some(ref cache_dir) = cli.cache_dir {
		rs = rs.with_cache_dir(std::path::PathBuf::from(cache_dir));
	}

	if cli.list {
		return run_list(cli, &rs);
	}

	if let Some(query) = cli.search.as_deref() {
		return run_search(cli, &rs, query);
	}

	let output = if cli.raw {
		rs.raw_json(
			&cli.target,
			cli.no_default_features,
			cli.all_features,
			cli.features.clone(),
			cli.private,
		)?
	} else {
		rs.render(
			&cli.target,
			cli.no_default_features,
			cli.all_features,
			cli.features.clone(),
			cli.private,
		)?
	};

	println!("{output}");

	Ok(())
}

/// Resolve the active search domains specified by the CLI flags.
fn search_domains_from_cli(cli: &Cli) -> SearchDomain {
	if cli.search_spec.is_empty() {
		SearchDomain::default()
	} else {
		cli.search_spec
			.iter()
			.fold(SearchDomain::empty(), |mut acc, spec| {
				acc |= SearchDomain::from(*spec);
				acc
			})
	}
}

/// Build a `SearchOptions` value using the provided CLI configuration and query.
fn build_search_options(cli: &Cli, query: &str) -> SearchOptions {
	let mut options = SearchOptions::new(query);
	options.include_private = cli.private;
	options.case_sensitive = cli.search_case_sensitive;
	options.expand_containers = !cli.direct_match_only;
	options.domains = search_domains_from_cli(cli);
	options
}

/// Execute the list flow and print a structured item summary.
fn run_list(cli: &Cli, rs: &Ripdoc) -> Result<(), Box<dyn Error>> {
	if cli.raw {
		return Err("--raw cannot be combined with --list".into());
	}

	let mut search_options: Option<SearchOptions> = None;
	let mut trimmed_query: Option<String> = None;

	if let Some(query) = cli.search.as_deref() {
		let trimmed = query.trim();
		if trimmed.is_empty() {
			println!("Search query is empty; nothing to do.");
			return Ok(());
		}
		trimmed_query = Some(trimmed.to_string());
		search_options = Some(build_search_options(cli, trimmed));
	}

	let listings = rs.list(
		&cli.target,
		cli.no_default_features,
		cli.all_features,
		cli.features.clone(),
		cli.private,
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

/// Highlight all occurrences of the search query in the output text with red color.
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
		result.push_str(&matched_text.to_string().red().to_string());
		last_end = match_end;
		search_start = match_end;
	}

	// Add remaining text
	result.push_str(&text[last_end..]);
	result
}

/// Execute the search flow and print the filtered skeleton to stdout.
fn run_search(cli: &Cli, rs: &Ripdoc, query: &str) -> Result<(), Box<dyn Error>> {
	if cli.raw {
		return Err("--raw cannot be combined with --search".into());
	}

	let trimmed = query.trim();
	if trimmed.is_empty() {
		println!("Search query is empty; nothing to do.");
		return Ok(());
	}

	let options = build_search_options(cli, trimmed);

	let response = rs.search(
		&cli.target,
		cli.no_default_features,
		cli.all_features,
		cli.features.clone(),
		&options,
	)?;

	if response.results.is_empty() {
		println!("No matches found for \"{}\".", trimmed);
		return Ok(());
	}

	let output = highlight_matches(&response.rendered, trimmed, cli.search_case_sensitive);

	print!("{}", output);

	Ok(())
}

fn main() {
	let cli = Cli::parse();
	let result = {
		if let Err(e) = check_nightly_toolchain() {
			eprintln!("{e}");
			process::exit(1);
		}
		run_cmdline(&cli)
	};

	if let Err(e) = result {
		eprintln!("{e}");
		process::exit(1);
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
