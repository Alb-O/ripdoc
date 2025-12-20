use rustdoc_types::{Id, Item, ItemEnum, VariantKind};
use super::super::impls::{render_impl, should_render_impl};
use super::super::state::{GapController, RenderState};
use super::super::syntax::*;
use super::super::utils::must_get;
use super::structs::render_struct_field;
use super::{SelectionView, collect_inline_traits};

/// Shared context for rendering enums and their variants consistently.
pub(crate) struct EnumRenderContext {
	generics: String,
	where_clause: String,
	selection: SelectionView,
}

impl EnumRenderContext {
	pub fn new(state: &RenderState, item: &Item, generics: String, where_clause: String) -> Self {
		Self {
			generics,
			where_clause,
			selection: SelectionView::new(state, &item.id, true),
		}
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

	pub fn should_render_variant(&self, state: &RenderState, variant_id: &Id) -> bool {
		!self.selection().is_active()
			|| self.selection().expands_self()
			|| self.selection().includes_child(state, variant_id)
	}

	pub fn include_variant_fields(&self, state: &RenderState, variant: &Item) -> bool {
		self.selection().expands_self()
			|| !self.selection().is_active()
			|| state.selection_matches(&variant.id)
	}
}

/// Render an enum definition, including variants.
pub fn render_enum(state: &mut RenderState, path_prefix: &str, item: &Item) -> String {
	let enum_ = extract_item!(item, ItemEnum::Enum);

	if !state.selection_context_contains(&item.id) {
		return String::new();
	}

	let rendered_enum = if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		crate::render::utils::extract_source(span, state.config.source_root.as_deref()).ok().map(|s| format!("{s}\n\n"))
	} else {
		let mut output = docs(item);

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
		Some(output)
	};

	let mut output = rendered_enum.unwrap_or_default();

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
	if state.selection_is_full_source(&item.id) && let Some(span) = &item.span {
		if let Ok(source) = crate::render::utils::extract_source(span, state.config.source_root.as_deref()) {
			return format!("    {source},\n");
		}
	}

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
