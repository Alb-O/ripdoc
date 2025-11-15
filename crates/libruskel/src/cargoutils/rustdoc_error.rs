use super::config::is_rustup_available;
use crate::error::RuskelError;

/// Maximum number of characters from rustdoc stderr included in failure reports.
const MAX_STDERR_CHARS: usize = 8_192;

/// Translate a `rustdoc_json` build failure into a user-facing [`RuskelError`].
pub fn map_rustdoc_build_error(
	err: &rustdoc_json::BuildError,
	captured_stderr: &[u8],
	silent: bool,
) -> RuskelError {
	match err {
		rustdoc_json::BuildError::BuildRustdocJsonError => {
			format_rustdoc_failure(captured_stderr, silent)
		}
		other => {
			let err_msg = other.to_string();
			let stderr_str = String::from_utf8_lossy(captured_stderr);

			if err_msg.contains("toolchain") && err_msg.contains("is not installed") {
				let install_msg = if is_rustup_available() {
					"run 'rustup toolchain install nightly'"
				} else {
					"ensure nightly Rust is installed and available in PATH"
				};
				return RuskelError::Generate(format!(
					"ruskel requires the nightly toolchain to be installed - {install_msg}"
				));
			}

			// Check for nightly feature compatibility issues
			if stderr_str.contains("unknown feature") || stderr_str.contains("E0635") {
				return RuskelError::Generate(format!(
					"Failed to build rustdoc JSON: This crate or its dependencies use unstable features that are not compatible with your current nightly toolchain.\n\
                    \nOriginal error: {err_msg}"
				));
			}

			if err_msg.contains("Failed to build rustdoc JSON") {
				return format_rustdoc_failure(captured_stderr, silent);
			}

			RuskelError::Generate(format!("Failed to build rustdoc JSON: {err_msg}"))
		}
	}
}

/// Format a detailed error for rustdoc build failures, optionally embedding diagnostics.
fn format_rustdoc_failure(captured_stderr: &[u8], silent: bool) -> RuskelError {
	let stderr_raw = String::from_utf8_lossy(captured_stderr).into_owned();
	let stderr_trimmed = stderr_raw.trim();

	// Check for nightly feature compatibility issues
	if stderr_trimmed.contains("unknown feature") || stderr_trimmed.contains("E0635") {
		return RuskelError::Generate(
            "Failed to build rustdoc JSON: This crate or its dependencies use unstable features that are not compatible with your current nightly toolchain.\n".to_string()
        );
	}

	let summary = extract_primary_diagnostic(stderr_trimmed).unwrap_or_else(|| {
		"rustdoc exited with an error; rerun with --verbose for full diagnostics.".to_string()
	});
	let summary = summary.trim();

	if silent {
		if stderr_trimmed.is_empty() {
			return RuskelError::Generate(
                "Failed to build rustdoc JSON: rustdoc exited with an error but emitted no diagnostics. \
                 Re-run with --verbose or `cargo rustdoc` to inspect the failure.".to_string(),
            );
		}

		let (diagnostics, truncated) = truncate_diagnostics(stderr_trimmed);
		let mut message = format!("Failed to build rustdoc JSON: {summary}");
		message.push_str("\n\nrustdoc stderr:\n");
		message.push_str(&diagnostics);
		if truncated {
			message.push_str("\n… output truncated …");
		}
		return RuskelError::Generate(message);
	}

	RuskelError::Generate(format!("Failed to build rustdoc JSON: {summary}"))
}

/// Extract the first meaningful rustdoc diagnostic from the captured stderr stream.
fn extract_primary_diagnostic(stderr: &str) -> Option<String> {
	let mut lines = stderr.lines().peekable();

	while let Some(line) = lines.next() {
		if !is_primary_error_line(line) {
			continue;
		}

		let mut snippet = vec![line.trim_end().to_string()];

		while let Some(peek) = lines.peek() {
			let trimmed = peek.trim_end();
			if trimmed.is_empty() {
				lines.next();
				break;
			}

			let trimmed_start = trimmed.trim_start_matches(' ');
			let is_line_number_block = trimmed.contains('|')
				&& trimmed
					.split_once('|')
					.map(|(prefix, _)| prefix.trim().chars().all(|c| c.is_ascii_digit()))
					.unwrap_or(false);

			let is_context_line = peek.starts_with(' ')
				|| peek.starts_with('\t')
				|| peek.starts_with('|')
				|| trimmed_start.starts_with("-->")
				|| trimmed_start.starts_with("note:")
				|| trimmed_start.starts_with("help:")
				|| trimmed_start.starts_with("warning:")
				|| trimmed_start.starts_with("= note:")
				|| trimmed_start.starts_with("= help:")
				|| trimmed_start.starts_with("= warning:")
				|| is_line_number_block;

			if !is_context_line {
				break;
			}

			snippet.push(lines.next().unwrap().trim_end().to_string());
		}

		return Some(snippet.join("\n"));
	}

	None
}

/// Determine whether a line introduces a new primary rustdoc error diagnostic.
fn is_primary_error_line(line: &str) -> bool {
	let trimmed = line.trim();

	if let Some(body) = trimmed.strip_prefix("error[") {
		return body.contains(']');
	}

	if let Some(body) = trimmed.strip_prefix("error:") {
		let body = body.trim_start();
		return !(body.starts_with("Compilation failed")
			|| body.starts_with("could not compile")
			|| body.starts_with("could not document"));
	}

	false
}

/// Truncate collected diagnostics to a manageable size, returning whether truncation occurred.
fn truncate_diagnostics(stderr: &str) -> (String, bool) {
	let mut buffer = String::new();
	let mut truncated = false;

	for (idx, ch) in stderr.chars().enumerate() {
		if idx >= MAX_STDERR_CHARS {
			truncated = true;
			break;
		}
		buffer.push(ch);
	}

	(buffer, truncated)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn primary_diagnostic_extracts_compiler_error() {
		let stderr = r#"
error: expected pattern, found `=`
 --> src/lib.rs:3:9
  |
3 |     let = left + right;
  |         ^ expected pattern

error: Compilation failed, aborting rustdoc
"#;

		let diagnostic =
			extract_primary_diagnostic(stderr).expect("should find primary diagnostic");
		assert!(diagnostic.contains("expected pattern"));
		assert!(diagnostic.contains("src/lib.rs:3:9"));
		assert!(!diagnostic.contains("Compilation failed"));
	}

	#[test]
	fn format_rustdoc_failure_includes_diagnostics_when_silent() {
		let stderr = b"error: expected pattern, found `=`\n --> src/lib.rs:3:9\n  |\n3 |     let = left + right;\n  |         ^ expected pattern\n";
		let message = format_rustdoc_failure(stderr, true).to_string();

		assert!(message.contains("Failed to build rustdoc JSON"));
		assert!(message.contains("expected pattern"));
		assert!(message.contains("src/lib.rs:3:9"));
		assert!(message.contains("rustdoc stderr"));
	}
}
