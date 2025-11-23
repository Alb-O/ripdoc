use rustdoc_types::{Id, Item, ItemEnum, StructKind, VariantKind, Visibility};

use super::impls::{DERIVE_TRAITS, render_impl, should_render_impl};
use super::macros::{render_macro, render_proc_macro};
use super::state::{GapController, RenderState};
use super::syntax::*;
use super::utils::{escape_path, must_get, ppush};

/// Captures how the current selection affects an item's children.
pub(crate) struct SelectionView {
	active: bool,
	expands_self: bool,
}

impl SelectionView {
	pub(crate) fn new(state: &RenderState, id: &Id, expands_when_inactive: bool) -> Self {
		let active = state.selection().is_some();
		let expands_self = if active {
			state.selection_expands(id)
		} else {
			expands_when_inactive
		};
		Self {
			active,
			expands_self,
		}
	}

	pub(crate) fn includes_child(&self, state: &RenderState, child_id: &Id) -> bool {
		if !self.active {
			return true;
		}
		self.expands_self || state.selection_context_contains(child_id)
	}

	fn force_children(&self) -> bool {
		self.active && self.expands_self
	}

	fn is_active(&self) -> bool {
		self.active
	}

	fn expands_self(&self) -> bool {
		self.expands_self
	}
}

/// Shared context for rendering structs with consistent generics/selection info.
struct StructRenderContext<'a> {
	item: &'a Item,
	generics: String,
	where_clause: String,
	selection: SelectionView,
}

impl<'a> StructRenderContext<'a> {
	fn new(state: &RenderState, item: &'a Item, generics: String, where_clause: String) -> Self {
		Self {
			item,
			generics,
			where_clause,
			selection: SelectionView::new(state, &item.id, false),
		}
	}

	fn item(&self) -> &Item {
		self.item
	}

	fn generics(&self) -> &str {
		&self.generics
	}

	fn where_clause(&self) -> &str {
		&self.where_clause
	}

	fn selection(&self) -> &SelectionView {
		&self.selection
	}

	fn force_children(&self) -> bool {
		self.selection.force_children()
	}
}

/// Shared context for rendering enums and their variants consistently.
struct EnumRenderContext {
	generics: String,
	where_clause: String,
	selection: SelectionView,
}

impl EnumRenderContext {
	fn new(state: &RenderState, item: &Item, generics: String, where_clause: String) -> Self {
		Self {
			generics,
			where_clause,
			selection: SelectionView::new(state, &item.id, true),
		}
	}

	fn generics(&self) -> &str {
		&self.generics
	}

	fn where_clause(&self) -> &str {
		&self.where_clause
	}

	fn selection(&self) -> &SelectionView {
		&self.selection
	}

	fn should_render_variant(&self, state: &RenderState, variant_id: &Id) -> bool {
		!self.selection().is_active()
			|| self.selection().expands_self()
			|| self.selection().includes_child(state, variant_id)
	}

	fn include_variant_fields(&self, state: &RenderState, variant: &Item) -> bool {
		self.selection().expands_self()
			|| !self.selection().is_active()
			|| state.selection_matches(&variant.id)
	}
}

/// Collect trait names rendered via `#[derive]` for the provided impl list.
fn collect_inline_traits<'a>(state: &'a RenderState, impls: &[Id]) -> Vec<&'a str> {
	let mut inline_traits = Vec::new();
	for impl_id in impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if impl_.is_synthetic {
			continue;
		}

		if let Some(trait_) = &impl_.trait_
			&& let Some(name) = trait_.path.split("::").last()
			&& DERIVE_TRAITS.contains(&name)
		{
			inline_traits.push(name);
		}
	}
	inline_traits
}

/// Render an item into Rust source text.
pub fn render_item(
	state: &mut RenderState,
	path_prefix: &str,
	item: &Item,
	force_private: bool,
) -> String {
	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	if state.should_filter(path_prefix, item) {
		return String::new();
	}

	let output = match &item.inner {
		ItemEnum::Module(_) => render_module(state, path_prefix, item),
		ItemEnum::Struct(_) => render_struct(state, path_prefix, item),
		ItemEnum::Enum(_) => render_enum(state, path_prefix, item),
		ItemEnum::Trait(_) => super::impls::render_trait(state, item),
		ItemEnum::Use(_) => render_use(state, path_prefix, item),
		ItemEnum::Function(_) => render_function_item(state, item, false),
		ItemEnum::Constant { .. } => render_constant_item(state, item),
		ItemEnum::TypeAlias(_) => render_type_alias_item(state, item),
		ItemEnum::Macro(_) => render_macro(item),
		ItemEnum::ProcMacro(_) => render_proc_macro(item),
		_ => String::new(),
	};

	if !force_private && !is_visible(state, item) {
		String::new()
	} else {
		output
	}
}

/// Render a module and its children.
pub fn render_module(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let path_prefix = ppush(path_prefix, &render_name(item));
	let mut output = format!("{}mod {} {{\n", render_vis(item), render_name(item));
	// Add module doc comment if present
	if state.should_module_doc(&path_prefix, item)
		&& let Some(docs) = &item.docs
	{
		for line in docs.lines() {
			output.push_str(&format!("    //! {line}\n"));
		}
		output.push('\n');
	}

	let module = extract_item!(item, ItemEnum::Module);
	let gaps = GapController::new("");
	gaps.begin_section(state);

	for item_id in &module.items {
		let child_item = must_get(state.crate_data, item_id);

		let rendered = render_item(state, &path_prefix, child_item, false);
		if !rendered.is_empty() {
			gaps.emit_if_needed(state, &mut output, &rendered);
			output.push_str(&rendered);
		} else {
			// Item was filtered out, mark it as skipped
			state.mark_skipped();
		}
	}

	output.push_str("}\n\n");
	output
}

/// Render a struct declaration and its fields.
pub fn render_struct(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let docs = docs(item);

	let struct_ = extract_item!(item, ItemEnum::Struct);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let generics = render_generics(&struct_.generics);
	let where_clause = render_where_clause(&struct_.generics);

	// Collect inline traits first while we have immutable access
	let inline_traits: Vec<String> = collect_inline_traits(state, &struct_.impls)
		.into_iter()
		.map(|s| s.to_string())
		.collect();

	let ctx = StructRenderContext::new(state, item, generics, where_clause);

	let rendered_struct = match &struct_.kind {
		StructKind::Unit => Some(render_struct_unit(&ctx)),
		StructKind::Tuple(fields) => render_struct_tuple(state, &ctx, fields),
		StructKind::Plain { fields, .. } => Some(render_struct_plain(state, &ctx, fields)),
	};

	let mut output = String::new();

	if let Some(rendered) = rendered_struct {
		output.push_str(&docs);
		if !inline_traits.is_empty() {
			output.push_str(&format!("#[derive({})]\n", inline_traits.join(", ")));
		}
		output.push_str(&rendered);
	}

	// Render impl blocks
	for impl_id in &struct_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if should_render_impl(impl_, state.config.render_auto_impls)
			&& state.selection_allows_child(&item.id, impl_id)
		{
			output.push_str(&render_impl(state, path_prefix, impl_item));
		}
	}

	output
}

fn render_struct_unit(ctx: &StructRenderContext) -> String {
	format!(
		"{}struct {}{}{};\n\n",
		render_vis(ctx.item()),
		render_name(ctx.item()),
		ctx.generics(),
		ctx.where_clause()
	)
}

fn render_struct_tuple(
	state: &RenderState,
	ctx: &StructRenderContext,
	fields: &[Option<Id>],
) -> Option<String> {
	let selection = ctx.selection();
	let include_placeholders = !selection.is_active() || selection.force_children();
	let fields_str = fields
		.iter()
		.filter_map(|field| match field {
			Some(id) => {
				if !selection.includes_child(state, id) {
					return None;
				}
				let field_item = must_get(state.crate_data, id);
				let ty = extract_item!(field_item, ItemEnum::StructField);
				if !is_visible(state, field_item) {
					Some("_".to_string())
				} else {
					Some(format!("{}{}", render_vis(field_item), render_type(ty)))
				}
			}
			None => include_placeholders.then(|| "_".to_string()),
		})
		.collect::<Vec<_>>()
		.join(", ");

	if selection.expands_self() || !fields_str.is_empty() {
		Some(format!(
			"{}struct {}{}({}){};\n\n",
			render_vis(ctx.item()),
			render_name(ctx.item()),
			ctx.generics(),
			fields_str,
			ctx.where_clause()
		))
	} else {
		None
	}
}

fn render_struct_plain(
	state: &mut RenderState,
	ctx: &StructRenderContext,
	fields: &[Id],
) -> String {
	let mut output = format!(
		"{}struct {}{}{} {{\n",
		render_vis(ctx.item()),
		render_name(ctx.item()),
		ctx.generics(),
		ctx.where_clause()
	);
	let gaps = GapController::new("    ");
	gaps.begin_section(state);

	for field in fields {
		let rendered = render_struct_field(state, field, ctx.force_children());
		if !rendered.is_empty() {
			gaps.emit_if_needed(state, &mut output, &rendered);
			output.push_str(&rendered);
		} else {
			state.mark_skipped();
		}
	}

	output.push_str("}\n\n");
	output
}

/// Render a struct field, optionally forcing visibility.
pub fn render_struct_field(
	state: &RenderState,
	field_id: &rustdoc_types::Id,
	force: bool,
) -> String {
	let field_item = must_get(state.crate_data, field_id);

	if state.selection().is_some() && !force && !state.selection_context_contains(field_id) {
		return String::new();
	}

	if !(force || is_visible(state, field_item)) {
		return String::new();
	}

	let ty = extract_item!(field_item, ItemEnum::StructField);
	let mut out = String::new();
	out.push_str(&docs(field_item));
	out.push_str(&format!(
		"{}{}: {},\n",
		render_vis(field_item),
		render_name(field_item),
		render_type(ty)
	));
	out
}

/// Render an enum definition, including variants.
pub fn render_enum(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let mut output = docs(item);

	let enum_ = extract_item!(item, ItemEnum::Enum);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	// Collect inline traits first while we have immutable access
	let inline_traits: Vec<String> = collect_inline_traits(state, &enum_.impls)
		.into_iter()
		.map(|s| s.to_string())
		.collect();

	let ctx = EnumRenderContext::new(
		state,
		item,
		render_generics(&enum_.generics),
		render_where_clause(&enum_.generics),
	);

	if !inline_traits.is_empty() {
		output.push_str(&format!("#[derive({})]\n", inline_traits.join(", ")));
	}

	output.push_str(&format!(
		"{}enum {}{}{} {{\n",
		render_vis(item),
		render_name(item),
		ctx.generics(),
		ctx.where_clause()
	));
	let gaps = GapController::new("    ");
	gaps.begin_section(state);

	for variant_id in &enum_.variants {
		if !ctx.should_render_variant(state, variant_id) {
			state.mark_skipped();
			continue;
		}

		let variant_item = must_get(state.crate_data, variant_id);
		let include_variant_fields = ctx.include_variant_fields(state, variant_item);
		let rendered = render_enum_variant(state, &ctx, variant_item, include_variant_fields);
		if !rendered.is_empty() {
			gaps.emit_if_needed(state, &mut output, &rendered);
			output.push_str(&rendered);
		} else {
			state.mark_skipped();
		}
	}

	output.push_str("}\n\n");

	// Render impl blocks
	for impl_id in &enum_.impls {
		let impl_item = must_get(state.crate_data, impl_id);
		let impl_ = extract_item!(impl_item, ItemEnum::Impl);
		if should_render_impl(impl_, state.config.render_auto_impls)
			&& state.selection_allows_child(&item.id, impl_id)
		{
			output.push_str(&render_impl(state, path_prefix, impl_item));
		}
	}

	output
}

/// Render a single enum variant.
fn render_enum_variant(
	state: &RenderState,
	ctx: &EnumRenderContext,
	item: &Item,
	include_all_fields: bool,
) -> String {
	let mut output = docs(item);
	let variant = extract_item!(item, ItemEnum::Variant);

	output.push_str(&format!("    {}", render_name(item)));

	match &variant.kind {
		VariantKind::Plain => {}
		VariantKind::Tuple(fields) => {
			let fields_str = fields
				.iter()
				.filter_map(|field| {
					field.as_ref().and_then(|id| {
						if ctx.selection().is_active()
							&& !include_all_fields
							&& !state.selection_context_contains(id)
						{
							return None;
						}
						let field_item = must_get(state.crate_data, id);
						let ty = extract_item!(field_item, ItemEnum::StructField);
						Some(render_type(ty))
					})
				})
				.collect::<Vec<_>>()
				.join(", ");
			output.push_str(&format!("({fields_str})"));
		}
		VariantKind::Struct { fields, .. } => {
			output.push_str(" {\n");
			for field in fields {
				if !ctx.selection().is_active()
					|| include_all_fields
					|| state.selection_context_contains(field)
				{
					let rendered = render_struct_field(
						state,
						field,
						include_all_fields || !ctx.selection().is_active(),
					);
					if !rendered.is_empty() {
						output.push_str(&rendered);
					}
				}
			}
			output.push_str("    }");
		}
	}

	if let Some(discriminant) = &variant.discriminant {
		output.push_str(&format!(" = {}", discriminant.expr));
	}

	output.push_str(",\n");

	output
}

enum UseResolution {
	Items(Vec<Id>),
	Alias { source: String, alias: String },
	Simple(String),
}

/// Render a `use` statement, applying filter rules for private modules.
pub fn render_use(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let import = extract_item!(item, ItemEnum::Use);
	let resolution = resolve_use(state, import);

	match resolution {
		UseResolution::Items(items) => {
			let mut output = String::new();
			let gaps = GapController::new("");
			gaps.begin_section(state);
			let mut any_rendered = false;

			// Render all expanded items as a single group so we don't emit gap
			// markers between items that originated from the same `use`.
			for item_id in items {
				if let Some(item) = state.crate_data.index.get(&item_id) {
					let rendered = render_item(state, path_prefix, item, true);
					if !rendered.is_empty() {
						gaps.emit_if_needed(state, &mut output, &rendered);
						output.push_str(&rendered);
						any_rendered = true;
					} else if !any_rendered {
						// Track that we skipped something before the first render.
						state.mark_skipped();
					}
				}
			}
			// Prevent skipped items inside the expansion from queuing another
			// gap marker for the parent module.
			state.clear_pending_gap();

			output
		}
		UseResolution::Alias { source, alias } => {
			let mut output = docs(item);
			output.push_str(&format!("pub use {source} as {alias};\n"));
			output
		}
		UseResolution::Simple(source) => {
			let mut output = docs(item);
			output.push_str(&format!("pub use {source};\n"));
			output
		}
	}
}

fn resolve_use(state: &RenderState, import: &rustdoc_types::Use) -> UseResolution {
	if import.is_glob {
		return resolve_glob_use(state, import);
	}

	if let Some(imported_item) = import
		.id
		.as_ref()
		.and_then(|id| state.crate_data.index.get(id))
	{
		return UseResolution::Items(vec![imported_item.id]);
	}

	resolve_alias_use(import)
}

fn resolve_glob_use(state: &RenderState, import: &rustdoc_types::Use) -> UseResolution {
	let Some(source_id) = &import.id else {
		return UseResolution::Simple(format!("{}::*", escape_path(&import.source)));
	};
	let Some(source_item) = state.crate_data.index.get(source_id) else {
		return UseResolution::Simple(format!("{}::*", escape_path(&import.source)));
	};

	match &source_item.inner {
		ItemEnum::Module(module) => {
			let items = module
				.items
				.iter()
				.filter(|item_id| {
					state
						.crate_data
						.index
						.get(item_id)
						.map(|item| is_visible(state, item))
						.unwrap_or(false)
				})
				.cloned()
				.collect();
			UseResolution::Items(items)
		}
		ItemEnum::Enum(enum_) => {
			let items = enum_
				.variants
				.iter()
				.filter(|variant_id| {
					state
						.crate_data
						.index
						.get(variant_id)
						.map(|variant| is_visible(state, variant))
						.unwrap_or(false)
				})
				.cloned()
				.collect();
			UseResolution::Items(items)
		}
		_ => UseResolution::Simple(format!("{}::*", escape_path(&import.source))),
	}
}

fn resolve_alias_use(import: &rustdoc_types::Use) -> UseResolution {
	use super::syntax::is_reserved_word;

	let source = escape_path(&import.source);
	let last_segment = import.source.split("::").last().unwrap_or(&import.source);
	if import.name != last_segment {
		let escaped_name = if is_reserved_word(import.name.as_str()) {
			format!("r#{}", import.name)
		} else {
			import.name.clone()
		};
		UseResolution::Alias {
			source,
			alias: escaped_name,
		}
	} else {
		UseResolution::Simple(source)
	}
}

/// Determine whether an item should be rendered based on visibility settings.
fn is_visible(state: &RenderState, item: &Item) -> bool {
	state.config.render_private_items || matches!(item.visibility, Visibility::Public)
}

/// Render a function or method signature.
fn render_function_item(_state: &RenderState, item: &Item, is_trait_method: bool) -> String {
	let mut output = docs(item);
	let function = extract_item!(item, ItemEnum::Function);

	// Handle const, async, and unsafe keywords in the correct order
	let mut prefixes = Vec::new();
	if function.header.is_const {
		prefixes.push("const");
	}
	if function.header.is_async {
		prefixes.push("async");
	}
	if function.header.is_unsafe {
		prefixes.push("unsafe");
	}

	output.push_str(&format!(
		"{} {} fn {}{}({}){}{}",
		render_vis(item),
		prefixes.join(" "),
		render_name(item),
		render_generics(&function.generics),
		render_function_args(&function.sig),
		render_return_type(&function.sig),
		render_where_clause(&function.generics)
	));

	// Use semicolon for trait method declarations, empty body for implementations
	if is_trait_method && !function.has_body {
		output.push_str(";\n\n");
	} else {
		output.push_str(" {}\n\n");
	}

	output
}

/// Render a constant definition.
fn render_constant_item(_state: &RenderState, item: &Item) -> String {
	let mut output = docs(item);

	let (type_, const_) = extract_item!(item, ItemEnum::Constant { type_, const_ });
	output.push_str(&format!(
		"{}const {}: {} = {};\n\n",
		render_vis(item),
		render_name(item),
		render_type(type_),
		const_.expr
	));

	output
}

/// Render a type alias with generics, bounds, and visibility.
fn render_type_alias_item(_state: &RenderState, item: &Item) -> String {
	let type_alias = extract_item!(item, ItemEnum::TypeAlias);
	let mut output = docs(item);

	output.push_str(&format!(
		"{}type {}{}{}",
		render_vis(item),
		render_name(item),
		render_generics(&type_alias.generics),
		render_where_clause(&type_alias.generics),
	));

	output.push_str(&format!("= {};\n\n", render_type(&type_alias.type_)));

	output
}
