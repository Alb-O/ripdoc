use std::collections::HashSet;

use rustdoc_types::Id;

use super::index::SearchIndex;
use super::types::{SearchDomain, SearchItemKind, SearchResult};
use crate::render::RenderSelection;

/// Build a renderer selection set covering matches, their ancestors, and optionally their children.
pub fn build_render_selection(
	index: &SearchIndex,
	results: &[SearchResult],
	expand_containers: bool,
	full_source: HashSet<Id>,
) -> RenderSelection {
	let mut matches = HashSet::new();
	let mut context = HashSet::new();
	let mut expanded = HashSet::new();
	for result in results {
		matches.insert(result.item_id);
		context.insert(result.item_id);
		context.extend(result.ancestors.iter().copied());

		// Ensure that if an impl block is matched, its target struct is included in the context.
		// This prevents "orphaned" impl blocks in flattened output.
		if let Some(item) = index.crate_data().index.get(&result.item_id)
			&& let rustdoc_types::ItemEnum::Impl(impl_) = &item.inner
				&& let rustdoc_types::Type::ResolvedPath(path) = &impl_.for_ {
					context.insert(path.id);
					// Also include ancestors of the target struct
					if let Some(target_entry) = index.get(&path.id) {
						context.extend(target_entry.ancestors.iter().copied());
					}
				}
	}

	// Full-source items should still render even when there are no explicit search results.
	for id in &full_source {
		matches.insert(*id);
		context.insert(*id);
		if let Some(entry) = index.get(id) {
			context.extend(entry.ancestors.iter().copied());
		}
	}

	if expand_containers {
		let containers: HashSet<Id> = results
			.iter()
			.filter(|result| {
				matches!(
					result.kind,
					SearchItemKind::Crate
						| SearchItemKind::Module
						| SearchItemKind::Struct
						| SearchItemKind::Trait
				)
			})
			.map(|result| result.item_id)
			.collect();

		if !containers.is_empty() {
			expanded.extend(containers.iter().copied());
			let mut descendant_containers = HashSet::new();
			for entry in index.entries() {
				if let Some(pos) = entry
					.ancestors
					.iter()
					.position(|ancestor| containers.contains(ancestor))
				{
					context.insert(entry.item_id);
					for descendant in entry.ancestors.iter().skip(pos + 1) {
						context.insert(*descendant);
						descendant_containers.insert(*descendant);
					}
				}
			}
			expanded.extend(descendant_containers);
		}
	}

	RenderSelection::new(matches, context, expanded, full_source)
}

/// Format the set of matched domains into human-friendly labels.
pub fn describe_domains(domains: SearchDomain) -> Vec<&'static str> {
	let mut labels = Vec::new();
	if domains.contains(SearchDomain::NAMES) {
		labels.push("name");
	}
	if domains.contains(SearchDomain::DOCS) {
		labels.push("doc");
	}
	if domains.contains(SearchDomain::PATHS) {
		labels.push("path");
	}
	if domains.contains(SearchDomain::SIGNATURES) {
		labels.push("signature");
	}
	labels
}
