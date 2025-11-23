use once_cell::sync::Lazy;
use regex::Regex;
use rustdoc_types::{Item, ItemEnum, MacroKind};

use super::syntax::*;

/// Reusable pattern for removing placeholder bodies from macro output.
static MACRO_PLACEHOLDER_REGEX: Lazy<Regex> =
	Lazy::new(|| Regex::new(r"\}\s*\{\s*\.\.\.\s*\}\s*$").expect("valid macro fallback pattern"));

/// Render a macro_rules! definition.
pub fn render_macro(item: &Item) -> String {
	use super::syntax::is_reserved_word;

	let mut output = docs(item);

	let macro_def = extract_item!(item, ItemEnum::Macro);
	// Add #[macro_export] for public macros
	output.push_str("#[macro_export]\n");

	// Handle reserved keywords in macro names
	let macro_str = macro_def.to_string();

	// Fix rustdoc's incorrect rendering of new-style macro syntax
	// rustdoc produces "} {\n    ...\n}" which is invalid syntax
	// For new-style macros, we need to remove the extra block
	let fixed_macro_str =
		if macro_str.starts_with("macro ") && !macro_str.starts_with("macro_rules!") {
			// This is a new-style declarative macro
			// Look for the problematic pattern where we have "} { ... }" at the end
			if MACRO_PLACEHOLDER_REGEX.is_match(&macro_str) {
				// Remove the invalid "{ ... }" part, just end after the pattern
				MACRO_PLACEHOLDER_REGEX.replace(&macro_str, "}").to_string()
			} else {
				macro_str
			}
		} else {
			macro_str
		};

	if let Some(name_start) = fixed_macro_str.find("macro_rules!") {
		let prefix = &fixed_macro_str[..name_start + 12]; // "macro_rules!"
		let rest = &fixed_macro_str[name_start + 12..];

		// Find the macro name (skip whitespace)
		let trimmed = rest.trim_start();
		if let Some(name_end) = trimmed.find(|c: char| c.is_whitespace() || c == '{') {
			let name = &trimmed[..name_end];
			let suffix = &trimmed[name_end..];

			// Check if the name is a reserved word
			if is_reserved_word(name) {
				output.push_str(&format!("{prefix} r#{name}{suffix}\n"));
			} else {
				output.push_str(&fixed_macro_str);
				output.push('\n');
			}
		} else {
			output.push_str(&fixed_macro_str);
			output.push('\n');
		}
	} else {
		output.push_str(&fixed_macro_str);
		output.push('\n');
	}

	output
}

/// Render a procedural macro definition.
pub fn render_proc_macro(item: &Item) -> String {
	let mut output = docs(item);

	let fn_name = render_name(item);

	let proc_macro = extract_item!(item, ItemEnum::ProcMacro);
	match proc_macro.kind {
		MacroKind::Derive => {
			if !proc_macro.helpers.is_empty() {
				output.push_str(&format!(
					"#[proc_macro_derive({}, attributes({}))]\n",
					fn_name,
					proc_macro.helpers.join(", ")
				));
			} else {
				output.push_str(&format!("#[proc_macro_derive({fn_name})]\n"));
			}
		}
		MacroKind::Attr => {
			output.push_str("#[proc_macro_attribute]\n");
		}
		MacroKind::Bang => {
			output.push_str("#[proc_macro]\n");
		}
	}
	let (args, return_type) = match proc_macro.kind {
		MacroKind::Attr => (
			"attr: proc_macro::TokenStream, item: proc_macro::TokenStream",
			"proc_macro::TokenStream",
		),
		_ => ("input: proc_macro::TokenStream", "proc_macro::TokenStream"),
	};

	output.push_str(&format!("pub fn {fn_name}({args}) -> {return_type} {{}}\n"));

	output
}
