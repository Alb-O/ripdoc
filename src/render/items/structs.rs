use rustdoc_types::{Id, Item, ItemEnum, StructKind};
use super::super::impls::{render_impl, should_render_impl};
use super::super::state::{GapController, RenderState};
use super::super::syntax::*;
use super::super::utils::must_get;
use super::{SelectionView, collect_inline_traits, is_visible};

/// Shared context for rendering structs with consistent generics/selection info.
pub(crate) struct StructRenderContext<'a> {
	item: &'a Item,
	generics: String,
	where_clause: String,
	selection: SelectionView,
}

impl<'a> StructRenderContext<'a> {
	pub fn new(state: &RenderState, item: &'a Item, generics: String, where_clause: String) -> Self {
		Self {
			item,
			generics,
			where_clause,
			selection: SelectionView::new(state, &item.id, false),
		}
	}

	pub fn item(&self) -> &Item {
		self.item
	}

	pub fn generics(&self) -> &str {
		&self.generics
	}

	pub fn where_clause(&self) -> &str {
		&self.where_clause
	}

	pub fn selection(&self) -> &SelectionView {
		&self.selection
	}

	pub fn force_children(&self) -> bool {
		self.selection.force_children()
	}
}

/// Render a struct declaration and its fields.
pub fn render_struct(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let struct_ = extract_item!(item, ItemEnum::Struct);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let docs = docs(item);

	let rendered_struct = if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		crate::render::utils::extract_source(span, state.config.source_root.as_deref()).ok().map(|s| format!("{s}\n\n"))
	} else {
		let generics = render_generics(&struct_.generics);
		let where_clause = render_where_clause(&struct_.generics);

		// Collect inline traits first while we have immutable access
		let inline_traits: Vec<String> = collect_inline_traits(state, &struct_.impls)
			.into_iter()
			.map(|s| s.to_string())
			.collect();

		let ctx = StructRenderContext::new(state, item, generics, where_clause);

		let rendered = match &struct_.kind {
			StructKind::Unit => Some(render_struct_unit(&ctx)),
			StructKind::Tuple(fields) => render_struct_tuple(state, &ctx, fields),
			StructKind::Plain { fields, .. } => Some(render_struct_plain(state, &ctx, fields)),
		};

		rendered.map(|r| {
			let mut output = String::new();
			output.push_str(&docs);
			if !inline_traits.is_empty() {
				output.push_str(&format!("#[derive({})]\n", inline_traits.join(", ")));
			}
			output.push_str(&r);
			output
		})
	};

	let mut output = rendered_struct.unwrap_or_default();

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

	if state.selection_is_full_source(field_id) && let Some(span) = &field_item.span {
		if let Ok(source) = crate::render::utils::extract_source(span, state.config.source_root.as_deref()) {
			let trimmed = source.trim();
			let suffix = if trimmed.ends_with(',') { "\n" } else { ",\n" };
			return format!("{source}{suffix}");
		}
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
