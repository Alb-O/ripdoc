#![cfg(feature = "v2-ts")]

use std::collections::BTreeMap;

use crate::core_api::{RenderFormat, SearchItemKind, SourceLocation};

use super::entry::V2Entry;

#[derive(Default)]
struct ModNode {
	name: String,
	signature: Option<String>,
	source: Option<SourceLocation>,
	children: BTreeMap<String, ModNode>,
	items: Vec<V2Entry>,
}

impl ModNode {
	fn new(name: impl Into<String>) -> Self {
		Self {
			name: name.into(),
			signature: None,
			source: None,
			children: BTreeMap::new(),
			items: Vec::new(),
		}
	}
}

/// Render a signatures-only skeleton for the provided entries.
pub(crate) fn render_entries(format: RenderFormat, source_labels: bool, package_name: &str, crate_name: &str, entries: &[V2Entry]) -> String {
	let mut root = ModNode::new(crate_name);
	root.signature = Some(format!("pub mod {crate_name}"));

	for entry in entries {
		if entry.kind == SearchItemKind::Crate {
			continue;
		}

		let segments: Vec<&str> = entry.path.split("::").collect();
		if segments.first().copied() != Some(crate_name) || segments.len() < 2 {
			continue;
		}

		match entry.kind {
			SearchItemKind::Module => insert_module(&mut root, &segments[1..], entry),
			_ => insert_item(&mut root, &segments[1..], entry.clone()),
		}
	}

	let mut code = String::new();
	emit_mod(&root, 0, &mut code, source_labels, true);

	match format {
		RenderFormat::Rust => format!("// Package: {package_name}\n\n{code}"),
		RenderFormat::Markdown => {
			format!("# Package: {package_name}\n\n```rust\n{code}```\n")
		}
	}
}

fn insert_module(root: &mut ModNode, segments: &[&str], entry: &V2Entry) {
	let mut node = root;
	for segment in segments {
		node = node.children.entry((*segment).to_string()).or_insert_with(|| ModNode::new(*segment));
	}

	if node.signature.is_none() {
		node.signature = entry.signature.clone().or_else(|| Some(format!("pub mod {}", node.name)));
	}
	if node.source.is_none() {
		node.source = entry.source.clone();
	}
}

fn insert_item(root: &mut ModNode, segments: &[&str], entry: V2Entry) {
	if segments.is_empty() {
		return;
	}

	let (mods, _leaf) = segments.split_at(segments.len().saturating_sub(1));
	let mut node = root;
	for segment in mods {
		node = node.children.entry((*segment).to_string()).or_insert_with(|| {
			let mut child = ModNode::new(*segment);
			child.signature = Some(format!("pub mod {}", segment));
			child
		});
	}
	node.items.push(entry);
}

fn emit_mod(node: &ModNode, indent: usize, out: &mut String, source_labels: bool, is_root: bool) {
	if !is_root
		&& source_labels
		&& let Some(label) = format_source_label(node.source.as_ref())
	{
		indent_line(indent, out);
		out.push_str("// ");
		out.push_str(&label);
		out.push('\n');
	}

	let fallback_sig = if is_root {
		format!("pub mod {}", node.name)
	} else {
		format!("pub mod {}", node.name)
	};
	let sig = node.signature.as_deref().unwrap_or(&fallback_sig);
	indent_line(indent, out);
	out.push_str(sig);
	out.push_str(" {\n");

	for child in node.children.values() {
		emit_mod(child, indent + 1, out, source_labels, false);
		out.push('\n');
	}

	let mut items = node.items.clone();
	items.sort_by(|a, b| {
		let a_rank = kind_rank(a.kind);
		let b_rank = kind_rank(b.kind);
		a_rank.cmp(&b_rank).then_with(|| a.path.cmp(&b.path))
	});
	for item in items {
		emit_item(&item, indent + 1, out, source_labels);
	}

	indent_line(indent, out);
	out.push_str("}\n");
}

fn emit_item(entry: &V2Entry, indent: usize, out: &mut String, source_labels: bool) {
	if source_labels && let Some(label) = format_source_label(entry.source.as_ref()) {
		indent_line(indent, out);
		out.push_str("// ");
		out.push_str(&label);
		out.push('\n');
	}

	let line = render_item_line(entry);
	if line.is_empty() {
		return;
	}

	indent_line(indent, out);
	out.push_str(&line);
	out.push('\n');
}

fn render_item_line(entry: &V2Entry) -> String {
	match entry.kind {
		SearchItemKind::Function | SearchItemKind::ProcMacro => {
			let sig = entry.signature.as_deref().unwrap_or_default().trim();
			if sig.is_empty() {
				format!("fn {}() {{}}", entry.name())
			} else {
				format!("{sig} {{}}")
			}
		}
		SearchItemKind::Struct => {
			let sig = entry.signature.as_deref().unwrap_or_default().trim();
			if sig.is_empty() {
				return format!("struct {} {{}}", entry.name());
			}
			if sig.ends_with(';') {
				return sig.to_string();
			}

			if sig.contains('(') {
				return format!("{sig};");
			}

			format!("{sig} {{}}")
		}
		SearchItemKind::Enum => {
			let sig = entry.signature.as_deref().unwrap_or_default().trim();
			if sig.is_empty() {
				format!("enum {} {{}}", entry.name())
			} else {
				format!("{sig} {{}}")
			}
		}
		SearchItemKind::Trait => {
			let sig = entry.signature.as_deref().unwrap_or_default().trim();
			if sig.is_empty() {
				format!("trait {} {{}}", entry.name())
			} else {
				format!("{sig} {{}}")
			}
		}
		SearchItemKind::TypeAlias => {
			let mut sig = entry.signature.as_deref().unwrap_or_default().trim().to_string();
			if sig.is_empty() {
				return format!("type {} = ();", entry.name());
			}
			if !sig.trim_end().ends_with(';') {
				sig.push(';');
			}
			sig
		}
		SearchItemKind::Constant | SearchItemKind::Static => {
			let sig = entry.signature.as_deref().unwrap_or_default().trim();
			if sig.is_empty() {
				String::new()
			} else {
				let expr = sig.split_once(':').map(|(_, ty)| placeholder_for_type(ty.trim())).unwrap_or("0");
				format!("{sig} = {expr};")
			}
		}
		SearchItemKind::Macro => {
			let head = entry.signature.as_deref().unwrap_or_default().trim();
			if head.contains("macro_rules!") {
				format!("{head} {{ ($($tt:tt)*) => {{}}; }}")
			} else {
				format!("macro_rules! {} {{ ($($tt:tt)*) => {{}}; }}", entry.name())
			}
		}
		SearchItemKind::Module | SearchItemKind::Crate | SearchItemKind::Use => String::new(),
		_ => entry.signature.as_deref().unwrap_or_default().trim().to_string(),
	}
}

fn placeholder_for_type(ty: &str) -> &'static str {
	let compact: String = ty.chars().filter(|ch| !ch.is_whitespace()).collect();

	if compact.starts_with('&') && compact.ends_with("str") {
		return "\"\"";
	}

	match compact.as_str() {
		"bool" => "false",
		"char" => "'\\0'",
		"u8" => "0u8",
		"u16" => "0u16",
		"u32" => "0u32",
		"u64" => "0u64",
		"u128" => "0u128",
		"usize" => "0usize",
		"i8" => "0i8",
		"i16" => "0i16",
		"i32" => "0i32",
		"i64" => "0i64",
		"i128" => "0i128",
		"isize" => "0isize",
		"f32" => "0.0f32",
		"f64" => "0.0f64",
		_ => "0",
	}
}

fn kind_rank(kind: SearchItemKind) -> u8 {
	match kind {
		SearchItemKind::Struct => 10,
		SearchItemKind::Enum => 11,
		SearchItemKind::Trait => 12,
		SearchItemKind::TypeAlias => 13,
		SearchItemKind::Constant => 20,
		SearchItemKind::Static => 21,
		SearchItemKind::Macro => 30,
		SearchItemKind::ProcMacro => 40,
		SearchItemKind::Function => 41,
		_ => 100,
	}
}

fn format_source_label(source: Option<&SourceLocation>) -> Option<String> {
	let source = source?;
	let mut value = source.path.clone();
	if let Some(line) = source.line {
		value.push(':');
		value.push_str(&line.to_string());
	}
	Some(value)
}

fn indent_line(indent: usize, out: &mut String) {
	for _ in 0..indent {
		out.push('\t');
	}
}
