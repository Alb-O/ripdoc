use rustdoc_types::{Crate, Id, Item};

/// Retrieve an item from the crate index, panicking if it is missing.
pub fn must_get<'a>(crate_data: &'a Crate, id: &Id) -> &'a Item {
	crate_data.index.get(id).unwrap()
}

/// Append `name` to a path prefix using `::` separators.
pub fn ppush(path_prefix: &str, name: &str) -> String {
	if path_prefix.is_empty() {
		name.to_string()
	} else {
		format!("{path_prefix}::{name}")
	}
}

/// Escape reserved keywords in a path by adding raw identifier prefixes when needed.
pub fn escape_path(path: &str) -> String {
	use super::syntax::is_reserved_word;

	path.split("::")
		.map(|segment| {
			// Some keywords like 'crate', 'self', 'super' cannot be raw identifiers
			if segment == "crate" || segment == "self" || segment == "super" || segment == "Self" {
				segment.to_string()
			} else if is_reserved_word(segment) {
				format!("r#{}", segment)
			} else {
				segment.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("::")
}

/// Standard gap marker line used to indicate skipped items.
pub const GAP_MARKER: &str = "// ...";

/// Check whether the output already starts with a gap marker (ignoring indentation and leading
/// blank lines).
pub fn starts_with_gap(output: &str) -> bool {
	output
		.lines()
		.find(|line| !line.trim().is_empty())
		.map(|line| line.trim_start() == GAP_MARKER)
		.unwrap_or(false)
}

/// Check whether the current output already ends with a gap marker (ignoring indentation).
pub fn ends_with_gap(output: &str) -> bool {
	output
		.trim_end_matches('\n')
		.rsplit('\n')
		.next()
		.map(|line| line.trim_start() == GAP_MARKER)
		.unwrap_or(false)
}

/// Collapse consecutive gap markers to a single instance.
pub fn dedup_gap_markers(output: &str) -> String {
	let mut deduped = String::with_capacity(output.len());
	let mut in_gap_block = false;
	let mut emitted_blank_after_gap = false;

	for line in output.lines() {
		let is_gap = line.trim_start() == GAP_MARKER;
		let is_blank = line.trim().is_empty();

		if is_gap {
			if in_gap_block {
				continue;
			}
			in_gap_block = true;
			emitted_blank_after_gap = false;
			deduped.push_str(line);
			deduped.push('\n');
			continue;
		}

		if in_gap_block {
			if is_blank {
				if emitted_blank_after_gap {
					continue;
				}
				emitted_blank_after_gap = true;
				deduped.push_str(line);
				deduped.push('\n');
				continue;
			}
			in_gap_block = false;
		}

		deduped.push_str(line);
		deduped.push('\n');
	}

	deduped
}

/// Classification describing how a filter string matches a path.
#[derive(Debug, PartialEq)]
pub enum FilterMatch {
	/// The filter exactly matches the path.
	Hit,
	/// The filter matches a prefix of the path.
	Prefix,
	/// The filter matches a suffix of the path.
	Suffix,
	/// The filter does not match the path.
	Miss,
}

/// Extract source code from a file based on span information.
pub fn extract_source(
	span: &rustdoc_types::Span,
	source_root: Option<&std::path::Path>,
) -> std::io::Result<String> {
	let mut path = span.filename.clone();

	// Heuristic for finding the file, especially in workspaces
	if !path.exists() {
		if let Some(root) = source_root {
			let joined = root.join(&span.filename);
			if joined.exists() {
				path = joined;
			} else if span.filename.is_relative() {
				// Try stripping leading components if it might be relative to a workspace root
				// but we are in a package root.
				let mut components = span.filename.components();
				while components.next().is_some() {
					let candidate = root.join(components.as_path());
					if candidate.exists() {
						path = candidate;
						break;
					}
				}
			}
		}
	}

	let file_content = match std::fs::read_to_string(&path) {
		Ok(content) => content,
		Err(e) => {
			eprintln!("Warning: Failed to read source file {}: {}", path.display(), e);
			return Err(e);
		}
	};
	let lines: Vec<&str> = file_content.lines().collect();



	if span.begin.0 == 0 || span.begin.0 > lines.len() {
		return Ok(String::new());
	}

	let start_line = span.begin.0 - 1;
	let end_line = std::cmp::min(span.end.0, lines.len());

	let mut extracted = Vec::new();
	for i in start_line..end_line {
		extracted.push(lines[i]);
	}

	let result = extracted.join("\n");
	Ok(result)
}
