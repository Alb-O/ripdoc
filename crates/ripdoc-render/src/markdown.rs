use std::iter::Peekable;

/// Render formatted Rust source into Markdown by stripping the outer module and
/// converting doc comments + code fences into Markdown-friendly output.
pub fn render_markdown(source: &str) -> String {
	let without_outer = strip_outer_module(source);
	rust_to_markdown(&without_outer)
}

fn rust_to_markdown(source: &str) -> String {
	let mut markdown = String::new();
	let mut in_code_block = false;
	let mut need_gap_before_code = false;
	let mut code_buffer: Vec<String> = Vec::new();
	let mut lines = source.lines().peekable();

	while let Some(line) = lines.next() {
		let trimmed = line.trim_start();

		if is_doc_comment(trimmed) {
			let doc_block = collect_doc_block(line, &mut lines);
			let is_inner_doc = trimmed.starts_with("///");
			let inline_doc = in_code_block
				&& is_inner_doc
				&& doc_block.len() == 1
				&& !doc_block[0].1.trim().is_empty();

			if inline_doc {
				let indent = &doc_block[0].0;
				let text = doc_block[0].1.trim();
				code_buffer.push(format!("{indent}// {text}"));
			} else {
				flush_code_block(&mut markdown, &mut code_buffer, &mut need_gap_before_code);
				in_code_block = false;
				let doc_contains_text = render_doc_block(&doc_block, &mut markdown);
				need_gap_before_code = doc_contains_text;
			}
			continue;
		}

		if trimmed.is_empty() {
			if in_code_block {
				code_buffer.push(String::new());
			} else if !markdown.is_empty() && !markdown.ends_with('\n') {
				markdown.push('\n');
			}
			continue;
		}

		if !in_code_block {
			in_code_block = true;
		}

		code_buffer.push(line.to_string());
	}

	flush_code_block(&mut markdown, &mut code_buffer, &mut need_gap_before_code);

	let normalized = normalize_spacing(&markdown);
	normalized.trim().to_string()
}

fn strip_outer_module(source: &str) -> String {
	let trimmed = source.trim();
	let mut lines: Vec<&str> = trimmed.lines().collect();

	if lines.len() >= 2 {
		let first = lines.first().unwrap().trim();
		let last = lines.last().unwrap().trim();
		if first.starts_with("pub mod ") && first.ends_with('{') && last == "}" {
			lines.remove(lines.len() - 1);
			lines.remove(0);
			return format!("{}\n", lines.join("\n"));
		}
	}

	// Handle degenerate single-line modules like `pub mod foo {}` which are
	// emitted when there are no visible items in the crate.
	if lines.len() == 1 {
		let first = lines[0].trim();
		if first.starts_with("pub mod ") && first.ends_with('}') {
			if let (Some(open), Some(close)) = (first.find('{'), first.rfind('}')) {
				if first[open + 1..close].trim().is_empty() {
					return String::new();
				}
			}
		}
	}

	trimmed.to_string()
}

fn collect_doc_block<'a, I>(first_line: &'a str, lines: &mut Peekable<I>) -> Vec<(String, String)>
where
	I: Iterator<Item = &'a str>,
{
	let mut block = Vec::new();
	let mut current_line = first_line;
	loop {
		let trimmed = current_line.trim_start();
		let indent = current_line
			.chars()
			.take_while(|c| c.is_whitespace())
			.collect::<String>();
		let text = strip_doc_comment(trimmed).trim_end().to_string();
		block.push((indent, text));

		match lines.peek() {
			Some(next_line) if is_doc_comment(next_line.trim_start()) => {
				current_line = lines.next().unwrap();
			}
			_ => break,
		}
	}
	block
}

fn is_doc_comment(line: &str) -> bool {
	line.starts_with("///") || line.starts_with("//!")
}

fn strip_doc_comment(line: &str) -> &str {
	if let Some(rest) = line.strip_prefix("///") {
		rest.strip_prefix(' ').unwrap_or(rest)
	} else if let Some(rest) = line.strip_prefix("//!") {
		rest.strip_prefix(' ').unwrap_or(rest)
	} else {
		line
	}
}

fn is_list_item(line: &str) -> bool {
	let trimmed = line.trim_start();
	if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
		return true;
	}

	let mut chars = trimmed.chars().peekable();
	let mut saw_digit = false;
	while matches!(chars.peek(), Some(c) if c.is_ascii_digit()) {
		saw_digit = true;
		chars.next();
	}
	if !saw_digit {
		return false;
	}

	matches!(chars.next(), Some('.') | Some(')')) && matches!(chars.next(), Some(' ' | '\t'))
}

fn ensure_block_gap(markdown: &mut String) {
	if markdown.is_empty() || markdown.ends_with("\n\n") {
		return;
	}
	if markdown.ends_with('\n') {
		markdown.push('\n');
	} else {
		markdown.push('\n');
		markdown.push('\n');
	}
}

fn render_doc_block(doc_block: &[(String, String)], markdown: &mut String) -> bool {
	let mut fence_open = false;
	let mut contains_text = false;
	let mut paragraph = String::new();
	let mut in_list_block = false;

	for (_, text) in doc_block {
		let trimmed_end = text.trim_end();
		let trimmed_start = trimmed_end.trim_start();
		if trimmed_start.starts_with("```") {
			flush_paragraph(markdown, &mut paragraph, &mut contains_text);
			let lang = trimmed_start[3..].trim();
			if let Some(mapped) = normalize_doc_lang(lang) {
				if fence_open {
					markdown.push_str("```\n\n");
				} else {
					markdown.push_str("```");
					markdown.push_str(mapped);
					markdown.push('\n');
				}
			} else {
				markdown.push_str(trimmed_start);
				markdown.push('\n');
			}
			fence_open = !fence_open;
			in_list_block = false;
		} else if fence_open {
			if let Some(line_to_write) = unhide_doctest_line(trimmed_end) {
				markdown.push_str(&line_to_write);
				markdown.push('\n');
			}
		} else if trimmed_start.is_empty() {
			flush_paragraph(markdown, &mut paragraph, &mut contains_text);
			if in_list_block {
				ensure_block_gap(markdown);
				in_list_block = false;
			}
		} else if is_list_item(trimmed_start) {
			flush_paragraph(markdown, &mut paragraph, &mut contains_text);
			if !in_list_block {
				ensure_block_gap(markdown);
			}
			markdown.push_str(trimmed_end);
			markdown.push('\n');
			in_list_block = true;
			contains_text = true;
		} else {
			if in_list_block {
				ensure_block_gap(markdown);
				in_list_block = false;
			}
			if !paragraph.is_empty() {
				paragraph.push(' ');
			}
			paragraph.push_str(trimmed_start);
		}
	}

	flush_paragraph(markdown, &mut paragraph, &mut contains_text);

	if in_list_block {
		ensure_block_gap(markdown);
	}

	if fence_open {
		markdown.push_str("```\n\n");
	}

	contains_text
}

fn flush_code_block(
	markdown: &mut String,
	code_buffer: &mut Vec<String>,
	need_gap_before_code: &mut bool,
) {
	if code_buffer.is_empty() || code_buffer.iter().all(|line| line.trim().is_empty()) {
		code_buffer.clear();
		return;
	}

	if *need_gap_before_code && !markdown.is_empty() {
		if !markdown.ends_with('\n') {
			markdown.push('\n');
		}
		markdown.push('\n');
	}

	markdown.push_str("```rust\n");
	markdown.push_str(&dedent_lines(code_buffer));
	markdown.push_str("```\n\n");
	code_buffer.clear();
	*need_gap_before_code = false;
}

fn dedent_lines(lines: &[String]) -> String {
	let min_indent = lines
		.iter()
		.filter_map(|line| {
			if line.trim().is_empty() {
				None
			} else {
				Some(
					line.as_bytes()
						.iter()
						.take_while(|&&b| matches!(b, b' ' | b'\t'))
						.count(),
				)
			}
		})
		.min()
		.unwrap_or(0);

	let mut result = String::new();
	for line in lines {
		if line.trim().is_empty() {
			result.push('\n');
		} else {
			let trim_at = min_indent.min(line.len());
			result.push_str(&line[trim_at..]);
			result.push('\n');
		}
	}
	result
}

fn flush_paragraph(markdown: &mut String, paragraph: &mut String, contains_text: &mut bool) {
	let trimmed = paragraph.trim();
	if trimmed.is_empty() {
		paragraph.clear();
		return;
	}
	if !markdown.is_empty() && !markdown.ends_with('\n') {
		markdown.push('\n');
	}
	if !markdown.is_empty() {
		markdown.push('\n');
	}
	markdown.push_str(trimmed);
	markdown.push('\n');
	markdown.push('\n');
	*contains_text = true;
	paragraph.clear();
}

fn unhide_doctest_line(line: &str) -> Option<String> {
	let trimmed = line.trim_start();
	if trimmed.starts_with('#') {
		None
	} else {
		Some(line.to_string())
	}
}

fn normalize_spacing(input: &str) -> String {
	let mut result: Vec<String> = Vec::new();
	let lines: Vec<&str> = input.lines().collect();
	let mut in_fence = false;

	for (idx, line) in lines.iter().enumerate() {
		let trimmed = line.trim();
		if trimmed.starts_with("```") {
			if in_fence
				&& result
					.last()
					.map(|prev| prev.trim().is_empty())
					.unwrap_or(false)
			{
				result.pop();
			}
			result.push((*line).to_string());
			in_fence = !in_fence;
			continue;
		}

		let is_blank = trimmed.is_empty();
		if is_blank {
			if result
				.last()
				.map(|prev| prev.trim().is_empty())
				.unwrap_or(false)
			{
				continue;
			}
			let next_is_closing = in_fence
				&& lines
					.get(idx + 1)
					.map(|next| next.trim().starts_with("```"))
					.unwrap_or(false);
			if next_is_closing {
				continue;
			}
			result.push(String::new());
		} else {
			result.push((*line).to_string());
		}
	}

	result.join("\n")
}

fn normalize_doc_lang(lang: &str) -> Option<&'static str> {
	let primary = lang.split(',').next().unwrap_or("").trim();
	match primary {
		"" => Some("rust"),
		"rust" => Some("rust"),
		"no_run" | "compile_fail" | "should_panic" | "ignore" => Some("rust"),
		"text" => Some(""),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn doc_comments_are_lifted_outside_code() {
		let source = "\
/// example docs
pub struct Foo {
    /// field docs
    pub field: i32,
}
";

		let expected = r#"example docs

```rust
pub struct Foo {
    // field docs
    pub field: i32,
}
```"#;

		assert_eq!(rust_to_markdown(source), expected.trim());
	}

	#[test]
	fn preserves_blank_doc_lines() {
		let source = "\
///
/// multiple paragraphs
pub struct Foo;
";

		let expected = r#"

multiple paragraphs

```rust
pub struct Foo;
```"#;

		assert_eq!(rust_to_markdown(source), expected.trim());
	}

	#[test]
	fn closes_unbalanced_doc_fences() {
		let source = "\
/// # Example
///
/// ```
/// let markdown = \"**very** _important\".into();
pub fn set_input(&mut self) {}
";

		let expected = r#"# Example

```rust
let markdown = "**very** _important".into();
```

```rust
pub fn set_input(&mut self) {}
```"#;

		assert_eq!(rust_to_markdown(source), expected.trim());
	}

	#[test]
	fn removes_uniform_leading_indentation() {
		let source = "\
\tpub fn alpha() {}
\tpub fn beta() {}
";

		let expected = r#"```rust
pub fn alpha() {}
pub fn beta() {}
```"#;

		assert_eq!(rust_to_markdown(source), expected);
	}

	#[test]
	fn strips_outer_module_wrapper() {
		let source = "\
pub mod example {
    pub struct Inner;
}
";

		let stripped = strip_outer_module(source);
		assert_eq!(stripped.trim(), "pub struct Inner;");
	}

	#[test]
	fn hides_doctest_setup_lines() {
		let source = "\
/// ```
/// # fn helper() {}
/// let value = helper();
/// # assert_eq!(value, ());
/// ```
pub fn demo() {}
";

		let expected = r#"```rust
let value = helper();
```

```rust
pub fn demo() {}
```"#;

		assert_eq!(rust_to_markdown(source), expected);
	}

	#[test]
	fn normalizes_compile_fail_blocks() {
		let source = "\
/// ```compile_fail
/// fn main() {
///     panic!(\"oops\");
/// }
/// ```
pub fn demo() {}
";

		let expected = r#"```rust
fn main() {
    panic!("oops");
}
```

```rust
pub fn demo() {}
```"#;

		assert_eq!(rust_to_markdown(source), expected);
	}

	#[test]
	fn preserves_list_structure() {
		let source = "\
/// Shopping list
/// - Apples
/// - Bananas
///
/// Notes follow.
pub struct Cart;
";

		let expected = r#"Shopping list

- Apples
- Bananas

Notes follow.

```rust
pub struct Cart;
```"#;

		assert_eq!(rust_to_markdown(source), expected.trim());
	}
}
