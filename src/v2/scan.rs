#![cfg(feature = "v2-ts")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tree_house_bindings::Node;

use crate::core_api::error::RipdocError;
use crate::core_api::{ListItem, Result, SearchItemKind, SourceLocation};

use super::parse;
use super::source::V2Source;

/// Scan the module graph rooted at the crate entrypoint.
///
/// Includes crate root, module items, and free functions for now.
pub(crate) fn list_crate(source: &V2Source, include_private: bool) -> Result<Vec<ListItem>> {
	let mut out = Vec::new();

	let entry_display = display_file_path_abs(source, &source.entry_file);
	out.push(ListItem {
		kind: SearchItemKind::Crate,
		path: source.crate_name.clone(),
		source: Some(SourceLocation {
			path: entry_display,
			line: Some(1),
			column: None,
		}),
	});

	let root_dir = source
		.entry_file
		.parent()
		.ok_or_else(|| RipdocError::InvalidTarget("Entry file has no parent directory".to_string()))?
		.to_path_buf();

	let mut visited_files: HashSet<PathBuf> = HashSet::new();
	scan_module_file(
		source,
		&source.entry_file,
		&source.crate_name,
		&root_dir,
		true,
		include_private,
		&mut out,
		&mut visited_files,
	)?;

	Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn scan_module_file(
	source: &V2Source,
	file: &Path,
	path_prefix: &str,
	module_dir: &Path,
	scope_public: bool,
	include_private: bool,
	out: &mut Vec<ListItem>,
	visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
	let abs = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
	if !visited_files.insert(abs.clone()) {
		return Ok(());
	}

	let (text, tree) = parse::parse_file(&abs)?;
	let lines = LineIndex::new(&text);
	let file_display = display_file_path_abs(source, &abs);
	let root = tree.root_node();

	scan_scope(
		source,
		root,
		&abs,
		&file_display,
		&text,
		&lines,
		path_prefix,
		module_dir,
		scope_public,
		include_private,
		out,
		visited_files,
	)?;

	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_scope(
	source: &V2Source,
	scope: Node<'_>,
	current_file: &Path,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	module_dir: &Path,
	scope_public: bool,
	include_private: bool,
	out: &mut Vec<ListItem>,
	visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
	for child in scope.children() {
		match child.kind() {
			"mod_item" => {
				scan_mod(
					source,
					child,
					current_file,
					file_display,
					text,
					lines,
					path_prefix,
					module_dir,
					scope_public,
					include_private,
					out,
					visited_files,
				)?;
			}
			"function_item" => {
				scan_fn(
					child,
					file_display,
					text,
					lines,
					path_prefix,
					scope_public,
					include_private,
					out,
				);
			}
			_ => {}
		}
	}

	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_mod(
	source: &V2Source,
	node: Node<'_>,
	current_file: &Path,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	module_dir: &Path,
	scope_public: bool,
	include_private: bool,
	out: &mut Vec<ListItem>,
	visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
	let Some(name) = node_identifier_text(&node, text) else {
		return Ok(());
	};

	let is_pub = is_pub(&node, text);
	let reachable = scope_public && is_pub;
	let keep = include_private || reachable;
	let line = lines.line_of(node.start_byte() as usize);
	let mod_path = format!("{path_prefix}::{name}");

	if keep {
		out.push(ListItem {
			kind: SearchItemKind::Module,
			path: mod_path.clone(),
			source: Some(SourceLocation {
				path: file_display.to_string(),
				line: Some(line),
				column: None,
			}),
		});
	}

	if !include_private && !reachable {
		return Ok(());
	}

	let child_module_dir = module_dir.join(&name);

	if let Some(body) = node.children().find(|c| c.kind() == "declaration_list") {
		return scan_scope(
			source,
			body,
			current_file,
			file_display,
			text,
			lines,
			&mod_path,
			&child_module_dir,
			reachable,
			include_private,
			out,
			visited_files,
		);
	}

	let path_attr = mod_path_attr(&node, text);
	if let Some(mod_file) =
		resolve_external_mod_file(module_dir, current_file, &name, path_attr.as_deref())
	{
		scan_module_file(
			source,
			&mod_file,
			&mod_path,
			&child_module_dir,
			reachable,
			include_private,
			out,
			visited_files,
		)?;
	}

	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_fn(
	node: Node<'_>,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	scope_public: bool,
	include_private: bool,
	out: &mut Vec<ListItem>,
) {
	let Some(name) = node_identifier_text(&node, text) else {
		return;
	};

	let is_pub = is_pub(&node, text);
	let reachable = scope_public && is_pub;
	if !include_private && !reachable {
		return;
	}

	let line = lines.line_of(node.start_byte() as usize);
	let fn_path = format!("{path_prefix}::{name}");
	out.push(ListItem {
		kind: SearchItemKind::Function,
		path: fn_path,
		source: Some(SourceLocation {
			path: file_display.to_string(),
			line: Some(line),
			column: None,
		}),
	});
}

fn resolve_external_mod_file(
	module_dir: &Path,
	current_file: &Path,
	mod_name: &str,
	path_attr: Option<&str>,
) -> Option<PathBuf> {
	if let Some(attr) = path_attr {
		let p = PathBuf::from(attr);
		let base = current_file.parent().unwrap_or_else(|| Path::new("."));
		let abs = if p.is_absolute() { p } else { base.join(p) };
		if abs.exists() {
			return Some(abs);
		}
	}

	let direct = module_dir.join(format!("{mod_name}.rs"));
	if direct.exists() {
		return Some(direct);
	}

	let nested = module_dir.join(mod_name).join("mod.rs");
	if nested.exists() {
		return Some(nested);
	}

	None
}

fn is_pub(node: &Node<'_>, text: &str) -> bool {
	let Some(vis) = node.children().find(|c| c.kind() == "visibility_modifier") else {
		return false;
	};

	let s = slice_text(&vis, text).unwrap_or_default();
	s.trim() == "pub"
}

fn node_identifier_text(node: &Node<'_>, text: &str) -> Option<String> {
	let ident = node.children().find(|c| c.kind() == "identifier")?;
	Some(slice_text(&ident, text)?.to_string())
}

fn slice_text<'a>(node: &Node<'_>, text: &'a str) -> Option<&'a str> {
	let range = node.byte_range();
	text.get(range.start as usize..range.end as usize)
}

fn mod_path_attr(node: &Node<'_>, text: &str) -> Option<String> {
	for attr in node.children().filter(|c| c.kind() == "attribute_item") {
		let s = slice_text(&attr, text)?;
		let idx = s.find("path")?;
		let s2 = &s[idx..];
		let q1 = s2.find('"')? + idx;
		let rest = &s[(q1 + 1)..];
		let q2 = rest.find('"')? + (q1 + 1);
		return Some(s[(q1 + 1)..q2].to_string());
	}

	None
}

fn display_file_path_abs(source: &V2Source, abs_file: &Path) -> String {
	let rel = abs_file
		.strip_prefix(&source.root_dir)
		.unwrap_or(abs_file)
		.to_string_lossy()
		.replace('\\', "/");
	let prefix = source.source_prefix.replace('\\', "/");
	format!("{prefix}/{rel}")
}

struct LineIndex {
	starts: Vec<usize>,
}

impl LineIndex {
	fn new(text: &str) -> Self {
		let mut starts = vec![0];
		for (i, b) in text.as_bytes().iter().enumerate() {
			if *b == b'\n' {
				starts.push(i + 1);
			}
		}
		Self { starts }
	}

	fn line_of(&self, byte: usize) -> usize {
		let idx = self.starts.partition_point(|&s| s <= byte);
		idx.max(1)
	}
}
