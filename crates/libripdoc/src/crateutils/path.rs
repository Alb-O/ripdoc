use rustdoc_types::Path;

/// Render a type or module path into Rust source form.
pub fn render_path(path: &Path) -> String {
	let args = path
		.args
		.as_ref()
		.map(|args| super::generics::render_generic_args(args))
		.unwrap_or_default();
	format!("{}{}", path.path.replace("$crate::", ""), args)
}
