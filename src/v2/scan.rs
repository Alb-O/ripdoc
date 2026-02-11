#![cfg(feature = "v2-ts")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tree_house_bindings::Node;

use crate::core_api::error::RipdocError;
use crate::core_api::{Result, SearchItemKind, SourceLocation};

use super::entry::V2Entry;
use super::parse;
use super::source::V2Source;

#[derive(Debug, Clone)]
struct PendingUseExport {
	alias_path: String,
	target_path: String,
	public_api: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Vis {
	Private,
	Pub,
	Crate,
	Super,
	Self_,
	In(String),
	Unknown(String),
}

impl Vis {
	fn is_pub_api(&self) -> bool {
		matches!(self, Self::Pub)
	}

	fn is_reexportish(&self) -> bool {
		!matches!(self, Self::Private)
	}
}

/// Scan the module graph rooted at the crate entrypoint.
pub(crate) fn list_crate(source: &V2Source, include_private: bool) -> Result<Vec<V2Entry>> {
	let mut entries = Vec::new();
	let mut pending_uses = Vec::new();

	let entry_display = display_file_path_abs(source, &source.entry_file);
	entries.push(V2Entry {
		kind: SearchItemKind::Crate,
		path: source.crate_name.clone(),
		source: Some(SourceLocation {
			path: entry_display,
			line: Some(1),
			column: None,
		}),
		docs: None,
		signature: None,
		public_api: true,
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
		&mut entries,
		&mut visited_files,
		&mut pending_uses,
	)?;

	apply_pending_use_aliases(&mut entries, pending_uses, include_private);

	if !include_private {
		entries.retain(|entry| entry.public_api || entry.kind == SearchItemKind::Crate);
	}

	Ok(entries)
}

#[allow(clippy::too_many_arguments)]
fn scan_module_file(
	source: &V2Source,
	file: &Path,
	path_prefix: &str,
	module_dir: &Path,
	scope_public: bool,
	entries: &mut Vec<V2Entry>,
	visited_files: &mut HashSet<PathBuf>,
	pending_uses: &mut Vec<PendingUseExport>,
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
		entries,
		visited_files,
		pending_uses,
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
	entries: &mut Vec<V2Entry>,
	visited_files: &mut HashSet<PathBuf>,
	pending_uses: &mut Vec<PendingUseExport>,
) -> Result<()> {
	let mut pending_attrs: Vec<String> = Vec::new();

	for child in scope.children() {
		match child.kind() {
			"attribute_item" => {
				if let Some(attr_text) = slice_text(&child, text) {
					pending_attrs.push(attr_text.to_string());
				}
				continue;
			}
			"mod_item" => scan_mod(
				source,
				child,
				current_file,
				file_display,
				text,
				lines,
				path_prefix,
				module_dir,
				scope_public,
				entries,
				visited_files,
				pending_uses,
			)?,
			"function_item" => scan_function(&pending_attrs, child, file_display, text, lines, path_prefix, scope_public, entries),
			"macro_definition" => scan_macro(&pending_attrs, source, child, file_display, text, lines, path_prefix, scope_public, entries),
			"use_declaration" => scan_use_declaration(source, child, text, path_prefix, scope_public, pending_uses),
			"struct_item" => scan_simple_item(SearchItemKind::Struct, child, file_display, text, lines, path_prefix, scope_public, entries),
			"enum_item" => scan_simple_item(SearchItemKind::Enum, child, file_display, text, lines, path_prefix, scope_public, entries),
			"trait_item" => scan_simple_item(SearchItemKind::Trait, child, file_display, text, lines, path_prefix, scope_public, entries),
			"type_item" => scan_simple_item(SearchItemKind::TypeAlias, child, file_display, text, lines, path_prefix, scope_public, entries),
			"const_item" => scan_simple_item(SearchItemKind::Constant, child, file_display, text, lines, path_prefix, scope_public, entries),
			"static_item" => scan_simple_item(SearchItemKind::Static, child, file_display, text, lines, path_prefix, scope_public, entries),
			_ => {}
		}

		pending_attrs.clear();
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
	entries: &mut Vec<V2Entry>,
	visited_files: &mut HashSet<PathBuf>,
	pending_uses: &mut Vec<PendingUseExport>,
) -> Result<()> {
	let Some(name) = node_identifier_text(&node, text) else {
		return Ok(());
	};

	let vis = visibility(&node, text);
	let public_api = scope_public && vis.is_pub_api();
	let line = lines.line_of(node.start_byte() as usize);
	let mod_path = format!("{path_prefix}::{name}");

	entries.push(V2Entry {
		kind: SearchItemKind::Module,
		path: mod_path.clone(),
		source: Some(SourceLocation {
			path: file_display.to_string(),
			line: Some(line),
			column: None,
		}),
		docs: extract_docs(text, node.start_byte() as usize),
		signature: extract_signature(SearchItemKind::Module, &node, text),
		public_api,
	});

	let child_scope_public = public_api;
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
			child_scope_public,
			entries,
			visited_files,
			pending_uses,
		);
	}

	let path_attr = mod_path_attr(&node, text);
	if let Some(mod_file) = resolve_external_mod_file(module_dir, current_file, &name, path_attr.as_deref()) {
		scan_module_file(
			source,
			&mod_file,
			&mod_path,
			&child_module_dir,
			child_scope_public,
			entries,
			visited_files,
			pending_uses,
		)?;
	}

	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_function(
	leading_attrs: &[String],
	node: Node<'_>,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	scope_public: bool,
	entries: &mut Vec<V2Entry>,
) {
	let kind = if has_any_attribute(leading_attrs, &node, text, &["proc_macro", "proc_macro_derive", "proc_macro_attribute"]) {
		SearchItemKind::ProcMacro
	} else {
		SearchItemKind::Function
	};

	scan_simple_item(kind, node, file_display, text, lines, path_prefix, scope_public, entries);
}

#[allow(clippy::too_many_arguments)]
fn scan_macro(
	leading_attrs: &[String],
	source: &V2Source,
	node: Node<'_>,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	scope_public: bool,
	entries: &mut Vec<V2Entry>,
) {
	let Some(name) = node_identifier_text(&node, text) else {
		return;
	};

	let macro_export = has_any_attribute(leading_attrs, &node, text, &["macro_export"]);
	let vis = visibility(&node, text);
	let public_api = if macro_export { true } else { scope_public && vis.is_pub_api() };

	let base_path = if macro_export { source.crate_name.as_str() } else { path_prefix };
	let path = format!("{base_path}::{name}");

	entries.push(V2Entry {
		kind: SearchItemKind::Macro,
		path,
		source: Some(SourceLocation {
			path: file_display.to_string(),
			line: Some(lines.line_of(node.start_byte() as usize)),
			column: None,
		}),
		docs: extract_docs(text, node.start_byte() as usize),
		signature: extract_signature(SearchItemKind::Macro, &node, text),
		public_api,
	});
}

fn scan_use_declaration(source: &V2Source, node: Node<'_>, text: &str, path_prefix: &str, scope_public: bool, pending_uses: &mut Vec<PendingUseExport>) {
	let vis = visibility(&node, text);
	if !vis.is_reexportish() {
		return;
	}

	let Some(use_text) = slice_text(&node, text) else {
		return;
	};

	let export_public_api = scope_public && vis.is_pub_api();
	let exports = parse_pub_use_exports(use_text);
	for export in exports {
		let Some(target_path) = canonicalize_use_target(path_prefix, &source.crate_name, &export.target_segments) else {
			continue;
		};
		let Some(alias) = export.alias else {
			continue;
		};

		pending_uses.push(PendingUseExport {
			alias_path: format!("{path_prefix}::{alias}"),
			target_path,
			public_api: export_public_api,
		});
	}
}

#[allow(clippy::too_many_arguments)]
fn scan_simple_item(
	kind: SearchItemKind,
	node: Node<'_>,
	file_display: &str,
	text: &str,
	lines: &LineIndex,
	path_prefix: &str,
	scope_public: bool,
	entries: &mut Vec<V2Entry>,
) {
	let Some(name) = node_identifier_text(&node, text) else {
		return;
	};

	let vis = visibility(&node, text);
	let public_api = scope_public && vis.is_pub_api();

	entries.push(V2Entry {
		kind,
		path: format!("{path_prefix}::{name}"),
		source: Some(SourceLocation {
			path: file_display.to_string(),
			line: Some(lines.line_of(node.start_byte() as usize)),
			column: None,
		}),
		docs: extract_docs(text, node.start_byte() as usize),
		signature: extract_signature(kind, &node, text),
		public_api,
	});
}

fn resolve_external_mod_file(module_dir: &Path, current_file: &Path, mod_name: &str, path_attr: Option<&str>) -> Option<PathBuf> {
	if let Some(attr) = path_attr {
		let path = PathBuf::from(attr);
		let base = current_file.parent().unwrap_or_else(|| Path::new("."));
		let abs = if path.is_absolute() { path } else { base.join(path) };
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

fn visibility(node: &Node<'_>, text: &str) -> Vis {
	let Some(vis) = node.children().find(|c| c.kind() == "visibility_modifier") else {
		return Vis::Private;
	};

	let raw = slice_text(&vis, text).unwrap_or("").trim();
	if raw.is_empty() {
		return Vis::Private;
	}

	let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
	match compact.as_str() {
		"pub" => Vis::Pub,
		"pub(crate)" => Vis::Crate,
		"pub(super)" => Vis::Super,
		"pub(self)" => Vis::Self_,
		_ if compact.starts_with("pub(in") && compact.ends_with(')') => {
			let inner = compact["pub(in".len()..compact.len() - 1].to_string();
			Vis::In(inner)
		}
		_ => Vis::Unknown(compact),
	}
}

fn node_identifier_text(node: &Node<'_>, text: &str) -> Option<String> {
	let ident = node.children().find(|c| matches!(c.kind(), "identifier" | "type_identifier"))?;
	Some(slice_text(&ident, text)?.to_string())
}

fn slice_text<'a>(node: &Node<'_>, text: &'a str) -> Option<&'a str> {
	let range = node.byte_range();
	text.get(range.start as usize..range.end as usize)
}

fn has_any_attribute(leading_attrs: &[String], node: &Node<'_>, text: &str, needles: &[&str]) -> bool {
	if leading_attrs.iter().any(|raw| needles.iter().any(|needle| raw.contains(needle))) {
		return true;
	}

	node.children().filter(|c| c.kind() == "attribute_item").any(|attr| {
		slice_text(&attr, text)
			.map(|raw| needles.iter().any(|needle| raw.contains(needle)))
			.unwrap_or(false)
	})
}

fn mod_path_attr(node: &Node<'_>, text: &str) -> Option<String> {
	for attr in node.children().filter(|c| c.kind() == "attribute_item") {
		let attr_text = slice_text(&attr, text)?;
		let idx = attr_text.find("path")?;
		let s2 = &attr_text[idx..];
		let q1 = s2.find('"')? + idx;
		let rest = &attr_text[(q1 + 1)..];
		let q2 = rest.find('"')? + (q1 + 1);
		return Some(attr_text[(q1 + 1)..q2].to_string());
	}

	None
}

fn extract_signature(kind: SearchItemKind, node: &Node<'_>, text: &str) -> Option<String> {
	let start = node.start_byte() as usize;
	let end = node.end_byte() as usize;
	let node_text = text.get(start..end)?.trim();
	if node_text.is_empty() {
		return None;
	}

	let cut = match kind {
		SearchItemKind::Constant | SearchItemKind::Static => node_text.find('=').or_else(|| node_text.find(';')).unwrap_or(node_text.len()),
		SearchItemKind::Struct => node_text.find('{').unwrap_or(node_text.len()),
		SearchItemKind::TypeAlias => node_text.find(';').unwrap_or(node_text.len()),
		_ => node_text.find('{').or_else(|| node_text.find(';')).unwrap_or(node_text.len()),
	};

	let sig = node_text[..cut].trim();
	if sig.is_empty() { None } else { Some(sig.to_string()) }
}

/// Extract contiguous `///` and `//!` docs immediately above an item.
///
/// `#[...]` attribute lines between docs and item are skipped.
fn extract_docs(text: &str, start_byte: usize) -> Option<String> {
	if start_byte == 0 || start_byte > text.len() {
		return None;
	}

	let mut docs: Vec<String> = Vec::new();
	let item_line_start = text[..start_byte].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
	if item_line_start == 0 {
		return None;
	}
	let mut idx = item_line_start.saturating_sub(1);

	loop {
		if idx == 0 {
			break;
		}

		let prefix = &text[..idx];
		let line_start = prefix.rfind('\n').map(|pos| pos + 1).unwrap_or(0);
		let line = &text[line_start..idx];
		let trimmed = line.trim_start();

		idx = line_start.saturating_sub(1);

		if trimmed.is_empty() {
			break;
		}

		if trimmed.starts_with("#") && (trimmed.starts_with("#[") || trimmed.starts_with("#![")) {
			continue;
		}

		if let Some(rest) = trimmed.strip_prefix("///") {
			docs.push(rest.trim_start().to_string());
			continue;
		}

		if let Some(rest) = trimmed.strip_prefix("//!") {
			docs.push(rest.trim_start().to_string());
			continue;
		}

		break;
	}

	if docs.is_empty() {
		None
	} else {
		docs.reverse();
		Some(docs.join("\n"))
	}
}

fn display_file_path_abs(source: &V2Source, abs_file: &Path) -> String {
	let rel = abs_file.strip_prefix(&source.root_dir).unwrap_or(abs_file).to_string_lossy().replace('\\', "/");
	let prefix = source.source_prefix.replace('\\', "/");
	format!("{prefix}/{rel}")
}

fn apply_pending_use_aliases(entries: &mut Vec<V2Entry>, pending_uses: Vec<PendingUseExport>, include_private: bool) {
	let mut seen_paths: HashSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();

	for export in pending_uses {
		if !include_private && !export.public_api {
			continue;
		}
		if seen_paths.contains(&export.alias_path) {
			continue;
		}

		let Some(target) = entries.iter().find(|entry| entry.path == export.target_path) else {
			continue;
		};

		let mut alias = target.clone();
		alias.path = export.alias_path.clone();
		alias.public_api = export.public_api;
		entries.push(alias);
		seen_paths.insert(export.alias_path);
	}
}

#[derive(Debug, Clone)]
struct UseLeaf {
	target_segments: Vec<String>,
	alias: Option<String>,
}

fn parse_pub_use_exports(use_text: &str) -> Vec<UseLeaf> {
	let mut text = use_text.trim();
	if let Some(rest) = text.strip_prefix("pub") {
		text = rest.trim_start();
		if text.starts_with('(')
			&& let Some(close_idx) = text.find(')')
		{
			text = text[(close_idx + 1)..].trim_start();
		}
	}
	let Some(rest) = text.strip_prefix("use") else {
		return Vec::new();
	};
	text = rest.trim_start();
	if let Some(trimmed) = text.strip_suffix(';') {
		text = trimmed.trim();
	}

	expand_use_expr(text, Vec::new())
}

fn expand_use_expr(expr: &str, prefix: Vec<String>) -> Vec<UseLeaf> {
	let expr = expr.trim();
	if expr.is_empty() {
		return Vec::new();
	}

	if let Some((head, inner)) = split_brace_group(expr) {
		let mut full_prefix = prefix;
		full_prefix.extend(split_path_segments(head.trim_end_matches("::")));

		let mut out = Vec::new();
		for item in split_top_level_commas(inner) {
			let item = item.trim();
			if item.is_empty() {
				continue;
			}

			if item == "self" {
				if let Some(last) = full_prefix.last().cloned() {
					out.push(UseLeaf {
						target_segments: full_prefix.clone(),
						alias: Some(last),
					});
				}
				continue;
			}

			out.extend(expand_use_expr(item, full_prefix.clone()));
		}
		return out;
	}

	let (path_part, alias_part) = split_as_alias(expr);
	let mut target = prefix;
	target.extend(split_path_segments(path_part));
	if target.is_empty() {
		return Vec::new();
	}

	let alias = alias_part.or_else(|| target.last().cloned());
	vec![UseLeaf {
		target_segments: target,
		alias,
	}]
}

fn split_as_alias(expr: &str) -> (&str, Option<String>) {
	if let Some(idx) = expr.rfind(" as ") {
		let left = expr[..idx].trim();
		let right = expr[(idx + 4)..].trim();
		if !right.is_empty() {
			return (left, Some(right.to_string()));
		}
	}
	(expr.trim(), None)
}

fn split_path_segments(path: &str) -> Vec<String> {
	path.split("::").map(str::trim).filter(|seg| !seg.is_empty()).map(ToOwned::to_owned).collect()
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
	let mut parts = Vec::new();
	let mut depth = 0usize;
	let mut start = 0usize;

	for (idx, ch) in input.char_indices() {
		match ch {
			'{' => depth += 1,
			'}' => depth = depth.saturating_sub(1),
			',' if depth == 0 => {
				parts.push(input[start..idx].trim());
				start = idx + 1;
			}
			_ => {}
		}
	}

	if start <= input.len() {
		parts.push(input[start..].trim());
	}

	parts
}

fn split_brace_group(expr: &str) -> Option<(&str, &str)> {
	let mut depth = 0usize;
	let mut brace_start: Option<usize> = None;
	let mut brace_end: Option<usize> = None;

	for (idx, ch) in expr.char_indices() {
		match ch {
			'{' => {
				if depth == 0 {
					brace_start = Some(idx);
				}
				depth += 1;
			}
			'}' => {
				if depth == 0 {
					return None;
				}
				depth -= 1;
				if depth == 0 {
					brace_end = Some(idx);
					break;
				}
			}
			_ => {}
		}
	}

	let start = brace_start?;
	let end = brace_end?;
	Some((&expr[..start], &expr[(start + 1)..end]))
}

fn canonicalize_use_target(path_prefix: &str, crate_name: &str, segs: &[String]) -> Option<String> {
	if segs.is_empty() {
		return None;
	}

	let mut current: Vec<String> = path_prefix.split("::").map(ToOwned::to_owned).collect();
	if current.is_empty() {
		current.push(crate_name.to_string());
	}

	let out = match segs.first().map(String::as_str) {
		Some("crate") => {
			let mut base = vec![crate_name.to_string()];
			base.extend(segs.iter().skip(1).cloned());
			base
		}
		Some("self") => {
			let mut base = current;
			base.extend(segs.iter().skip(1).cloned());
			base
		}
		Some("super") => {
			let mut base = current;
			let mut idx = 0usize;
			while idx < segs.len() && segs[idx] == "super" {
				if base.len() > 1 {
					base.pop();
				}
				idx += 1;
			}
			base.extend(segs.iter().skip(idx).cloned());
			base
		}
		Some(first) if first == crate_name => segs.to_vec(),
		Some(_) => {
			let mut base = current;
			base.extend(segs.iter().cloned());
			base
		}
		None => return None,
	};

	Some(out.join("::"))
}

struct LineIndex {
	starts: Vec<usize>,
}

impl LineIndex {
	fn new(text: &str) -> Self {
		let mut starts = vec![0];
		for (idx, byte) in text.as_bytes().iter().enumerate() {
			if *byte == b'\n' {
				starts.push(idx + 1);
			}
		}
		Self { starts }
	}

	fn line_of(&self, byte: usize) -> usize {
		let idx = self.starts.partition_point(|&start| start <= byte);
		idx.max(1)
	}
}
