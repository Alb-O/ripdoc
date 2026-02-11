#![cfg(feature = "v2-ts")]

use std::ops::Range as StdRange;
use std::path::Path;

use regex_cursor::Cursor;
use tree_house_bindings::{Grammar, Input, Parser, Tree};

use crate::core_api::error::RipdocError;
use crate::core_api::Result;

pub(crate) fn parse_file(path: &Path) -> Result<(String, Tree)> {
	let text = std::fs::read_to_string(path).map_err(|e| {
		RipdocError::InvalidTarget(format!(
			"Failed to read source file '{}': {e}",
			path.display()
		))
	})?;
	let tree = parse_source(&text)?;
	Ok((text, tree))
}

pub(crate) fn parse_source(text: &str) -> Result<Tree> {
	let grammar = Grammar::try_from(tree_sitter_rust::LANGUAGE)
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to load Rust grammar: {e}")))?;
	let mut parser = Parser::new();
	parser
		.set_grammar(grammar)
		.map_err(|e| RipdocError::InvalidTarget(format!("Failed to set Rust grammar: {e}")))?;

	let input = StrInput::new(text);
	parser
		.parse(input, None)
		.ok_or_else(|| RipdocError::InvalidTarget("tree-sitter parse returned None".to_string()))
}

struct StrInput<'a> {
	bytes: &'a [u8],
	cursor: ByteCursor<'a>,
}

impl<'a> StrInput<'a> {
	fn new(text: &'a str) -> Self {
		let bytes = text.as_bytes();
		Self {
			bytes,
			cursor: ByteCursor::new(bytes),
		}
	}
}

impl<'a> Input for StrInput<'a> {
	type Cursor = ByteCursor<'a>;

	fn cursor_at(&mut self, offset: u32) -> &mut Self::Cursor {
		self.cursor.set_pos(self.bytes, offset as usize);
		&mut self.cursor
	}

	fn eq(&mut self, range1: StdRange<u32>, range2: StdRange<u32>) -> bool {
		let b = self.bytes;
		let r1 = (range1.start as usize)..(range1.end as usize);
		let r2 = (range2.start as usize)..(range2.end as usize);
		let s1 = b.get(r1);
		let s2 = b.get(r2);
		s1.is_some() && s1 == s2
	}
}

#[derive(Clone)]
struct ByteCursor<'a> {
	bytes: &'a [u8],
	offset: usize,
	chunk: &'a [u8],
}

impl<'a> ByteCursor<'a> {
	fn new(bytes: &'a [u8]) -> Self {
		Self {
			bytes,
			offset: 0,
			chunk: bytes,
		}
	}

	fn set_pos(&mut self, bytes: &'a [u8], requested: usize) {
		self.bytes = bytes;
		let len = bytes.len();
		if len == 0 {
			self.offset = 0;
			self.chunk = bytes;
			return;
		}

		let clamped = requested.min(len);
		self.offset = if clamped >= len { len - 1 } else { clamped };
		self.chunk = &bytes[self.offset..];
	}
}

impl<'a> Cursor for ByteCursor<'a> {
	fn chunk(&self) -> &[u8] {
		self.chunk
	}

	fn advance(&mut self) -> bool {
		false
	}

	fn backtrack(&mut self) -> bool {
		false
	}

	fn total_bytes(&self) -> Option<usize> {
		Some(self.bytes.len())
	}

	fn offset(&self) -> usize {
		self.offset
	}

	fn utf8_aware(&self) -> bool {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_smoke_fn() {
		let src = "pub fn hello() {}\n";
		let tree = parse_source(src).expect("parse");
		let root = tree.root_node();
		assert_eq!(root.kind(), "source_file");
		assert!(root.children().any(|n| n.kind() == "function_item"));
	}
}
