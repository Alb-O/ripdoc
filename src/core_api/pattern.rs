//! Pattern utilities for search query handling.
//!
//! Provides regex escaping and symbol stripping functions that preserve pipe characters
//! for OR search functionality.

/// Escape regex metacharacters except pipes, enabling literal matching with OR support.
///
/// Escapes all regex special characters (`.`, `*`, `+`, etc.) while preserving `|` as
/// the OR operator. This allows queries like `"foo.txt|bar*"` to match either literal
/// string "foo.txt" or "bar*" rather than treating `.` and `*` as regex wildcards.
pub fn escape_regex_preserving_pipes(pattern: &str) -> String {
	let mut escaped = String::with_capacity(pattern.len() * 2);
	for ch in pattern.chars() {
		match ch {
			'|' => escaped.push(ch),
			'\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' => {
				escaped.push('\\');
				escaped.push(ch);
			}
			_ => escaped.push(ch),
		}
	}
	escaped
}

/// Strip Rust syntax symbols, preserving alphanumerics, underscores, whitespace, and pipes.
///
/// Used to normalize signatures and documentation for matching, removing punctuation like
/// `::`, `&`, `->` while keeping pipes for OR searches. Converts `"fn foo() -> u32"` to
/// `"fn foo  u32"` and `"init|clone"` to `"init|clone"`.
pub fn strip_symbols_preserving_pipes(text: &str) -> String {
	text.chars()
		.filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_' || *c == '|')
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_escape_preserves_pipe() {
		assert_eq!(escape_regex_preserving_pipes("foo|bar"), "foo|bar");
		assert_eq!(escape_regex_preserving_pipes("a|b|c"), "a|b|c");
	}

	#[test]
	fn test_escape_escapes_special_chars() {
		assert_eq!(escape_regex_preserving_pipes("foo.bar"), "foo\\.bar");
		assert_eq!(escape_regex_preserving_pipes("foo*bar"), "foo\\*bar");
		assert_eq!(escape_regex_preserving_pipes("foo+bar"), "foo\\+bar");
		assert_eq!(escape_regex_preserving_pipes("foo?bar"), "foo\\?bar");
		assert_eq!(escape_regex_preserving_pipes("foo(bar)"), "foo\\(bar\\)");
		assert_eq!(escape_regex_preserving_pipes("foo[bar]"), "foo\\[bar\\]");
		assert_eq!(escape_regex_preserving_pipes("foo{bar}"), "foo\\{bar\\}");
		assert_eq!(escape_regex_preserving_pipes("^foo$"), "\\^foo\\$");
		assert_eq!(escape_regex_preserving_pipes("foo\\bar"), "foo\\\\bar");
	}

	#[test]
	fn test_escape_combined() {
		assert_eq!(
			escape_regex_preserving_pipes("foo.bar|baz*"),
			"foo\\.bar|baz\\*"
		);
		assert_eq!(
			escape_regex_preserving_pipes("init|clone|fetch.rs"),
			"init|clone|fetch\\.rs"
		);
	}

	#[test]
	fn test_strip_symbols_preserves_pipe() {
		assert_eq!(
			strip_symbols_preserving_pipes("init|clone|fetch"),
			"init|clone|fetch"
		);
	}

	#[test]
	fn test_strip_symbols_removes_special_chars() {
		assert_eq!(
			strip_symbols_preserving_pipes("fn foo(bar: &str) -> u32"),
			"fn foobar str  u32"
		);
		assert_eq!(
			strip_symbols_preserving_pipes("pub struct Foo { }"),
			"pub struct Foo  "
		);
	}

	#[test]
	fn test_strip_symbols_combined() {
		assert_eq!(
			strip_symbols_preserving_pipes("fn init()|fn clone()"),
			"fn init|fn clone"
		);
	}
}
