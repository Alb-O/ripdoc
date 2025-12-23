use std::path::{Path, PathBuf};

use super::state::SkeleEntry;
use crate::cargo_utils::resolve_target;
use crate::core_api::error::RipdocError;
use crate::core_api::search::{
	SearchDomain, SearchIndex, SearchItemKind, SearchOptions, SearchResult,
};
use crate::core_api::{Result, Ripdoc};

#[derive(Debug, Clone)]
/// Extra details discovered while validating a skelebuild add target.
pub struct ValidatedTargetInfo {
	/// Canonical path that rustdoc search matched.
	pub matched_path: String,
	/// Best-effort source location (`path:line`) when available.
	pub source_location: Option<String>,
	/// Best-effort span line count when available.
	pub span_line_count: Option<usize>,
}

/// Normalize a target specification for persistent storage.
///
/// If the target is a relative path, it is converted to an absolute path to ensure
/// the state remains valid even if ripdoc is executed from a different directory later.
pub fn normalize_target_spec_for_storage(target: &str) -> String {
	let parsed = crate::cargo_utils::target::Target::parse(target);
	let Ok(parsed) = parsed else {
		return target.to_string();
	};
	match parsed.entrypoint {
		crate::cargo_utils::target::Entrypoint::Path(path) => {
			let abs = if path.is_relative() {
				match std::path::absolute(&path) {
					Ok(abs) => abs,
					Err(_) => return target.to_string(),
				}
			} else {
				path
			};
			let mut spec = abs.to_string_lossy().to_string();
			if !parsed.path.is_empty() {
				spec.push_str("::");
				spec.push_str(&parsed.path.join("::"));
			}
			spec
		}
		crate::cargo_utils::target::Entrypoint::Name { .. } => target.to_string(),
	}
}

/// Generate a list of candidate path queries for a given base query and crate name.
pub fn build_query_candidates(base_query: &str, crate_name: Option<&str>) -> Vec<String> {
	let mut candidates: Vec<String> = vec![base_query.to_string()];
	if let Some((first, rest)) = base_query.split_once("::") {
		if let Some(crate_name) = crate_name
			&& first != crate_name
		{
			candidates.push(format!("{crate_name}::{rest}"));
		}
		candidates.push(rest.to_string());
	}
	candidates.dedup();
	candidates
}

/// Resolve the best matching item in the index for a given path query.
///
/// This performs a search across candidates and prefers local matches (source within `pkg_root`).
pub fn resolve_best_path_match(
	index: &SearchIndex,
	crate_name: Option<&str>,
	pkg_root: &Path,
	base_query: &str,
	is_local: impl Fn(&SearchResult) -> bool,
	include_private: bool,
) -> Option<SearchResult> {
	let candidates = build_query_candidates(base_query, crate_name);
	for candidate in candidates {
		let mut options = SearchOptions::new(&candidate);
		options.domains = SearchDomain::PATHS;
		options.include_private = include_private;
		let mut results = index.search(&options);
		if candidate.contains("::") {
			results.retain(|r| {
				r.path_string == candidate || r.path_string.ends_with(&format!("::{candidate}"))
			});
		}

		let mut local: Vec<SearchResult> =
			results.iter().filter(|&r| is_local(r)).cloned().collect();
		let pool = if !local.is_empty() {
			&mut local
		} else {
			&mut results
		};
		if pool.is_empty() {
			continue;
		}

		pool.sort_by_key(|r| {
			(
				!is_local(r),
				match r.kind {
					SearchItemKind::Struct
					| SearchItemKind::Enum
					| SearchItemKind::Trait
					| SearchItemKind::TraitAlias
					| SearchItemKind::TypeAlias
					| SearchItemKind::Function
					| SearchItemKind::Method => 0usize,
					SearchItemKind::Module => 1usize,
					_ => 2usize,
				},
				r.path_string.len(),
			)
		});

		if pool.len() > 1 {
			eprintln!(
				"Warning: ambiguous match for `{}` in `{}`; using `{}`",
				base_query,
				pkg_root.display(),
				pool[0].path_string
			);
		}
		return Some(pool[0].clone());
	}

	None
}

/// Resolve a target as an implementation block (e.g., `Type::Trait`).
pub fn resolve_impl_target(
	index: &SearchIndex,
	crate_data: &rustdoc_types::Crate,
	crate_name: Option<&str>,
	pkg_root: &Path,
	base_query: &str,
	is_local: impl Fn(&SearchResult) -> bool,
	include_private: bool,
) -> Option<(SearchResult, rustdoc_types::Id)> {
	let (type_query, trait_name) = base_query.rsplit_once("::")?;
	if trait_name.is_empty() {
		return None;
	}

	let ty_match = resolve_best_path_match(index, crate_name, pkg_root, type_query, &is_local, include_private)?;
	if !matches!(
		ty_match.kind,
		SearchItemKind::Struct | SearchItemKind::Enum | SearchItemKind::Union
	) {
		return None;
	}

	let mut trait_options = SearchOptions::new(trait_name);
	trait_options.domains = SearchDomain::NAMES | SearchDomain::PATHS;
	trait_options.include_private = include_private;
	let mut trait_results: Vec<SearchResult> = index
		.search(&trait_options)
		.into_iter()
		.filter(|r| matches!(r.kind, SearchItemKind::Trait | SearchItemKind::TraitAlias))
		.collect();
	if trait_results.is_empty() {
		return None;
	}
	trait_results.sort_by_key(|r| {
		(
			(r.raw_name != trait_name),
			!is_local(r),
			r.path_string.len(),
		)
	});
	let trait_match = trait_results.first()?.clone();

	let Some(ty_item) = crate_data.index.get(&ty_match.item_id) else {
		return None;
	};
	let impl_ids: Vec<rustdoc_types::Id> = match &ty_item.inner {
		rustdoc_types::ItemEnum::Struct(struct_) => struct_.impls.clone(),
		rustdoc_types::ItemEnum::Enum(enum_) => enum_.impls.clone(),
		rustdoc_types::ItemEnum::Union(union_) => union_.impls.clone(),
		_ => Vec::new(),
	};
	for impl_id in impl_ids {
		let Some(impl_item) = crate_data.index.get(&impl_id) else {
			continue;
		};
		let rustdoc_types::ItemEnum::Impl(impl_) = &impl_item.inner else {
			continue;
		};
		let Some(trait_path) = &impl_.trait_ else {
			continue;
		};
		if trait_path.id == trait_match.item_id {
			return Some((ty_match, impl_id));
		}
	}
	None
}

/// Validate that a target specification can be resolved against its crate.
pub fn validate_add_target_or_error(
	target_spec: &str,
	ripdoc: &Ripdoc,
	include_private: bool,
	strict: bool,
) -> Result<ValidatedTargetInfo> {
	let parsed = crate::cargo_utils::target::Target::parse(target_spec)?;
	if parsed.path.is_empty() {
		return Ok(ValidatedTargetInfo {
			matched_path: target_spec.to_string(),
			source_location: None,
			span_line_count: None,
		});
	}

	let base_query = match &parsed.entrypoint {
		crate::cargo_utils::target::Entrypoint::Name { name, .. } => {
			format!("{name}::{}", parsed.path.join("::"))
		}
		crate::cargo_utils::target::Entrypoint::Path(_) => parsed.path.join("::"),
	};

	let resolved = resolve_target(target_spec, ripdoc.offline())
		.map_err(|err| RipdocError::InvalidTarget(format!("{err}")))?;
	let rt = resolved
		.first()
		.ok_or_else(|| RipdocError::InvalidTarget("No resolved targets".to_string()))?;
	let pkg_root = rt.package_root().to_path_buf();
	let crate_data = rt.read_crate(
		false,
		false,
		vec![],
		true,
		ripdoc.silent(),
		ripdoc.cache_config(),
	)?;
	let index = SearchIndex::build(&crate_data, true, Some(&pkg_root));
	let crate_name = crate_data
		.index
		.get(&crate_data.root)
		.and_then(|root| root.name.clone());

	let resolve_span_path = |span: &rustdoc_types::Span| -> PathBuf {
		let mut path = span.filename.clone();
		if path.is_relative() {
			let joined = pkg_root.join(&path);
			if joined.exists() {
				path = joined;
			}
		}
		path
	};
	let is_local = |result: &SearchResult| -> bool {
		let Some(item) = crate_data.index.get(&result.item_id) else {
			return false;
		};
		let Some(span) = &item.span else {
			return false;
		};
		resolve_span_path(span).starts_with(&pkg_root)
	};

	let mut matched_path: Option<String> = None;
	let mut matched_id: Option<rustdoc_types::Id> = None;
	
	// Try original query first
	if let Some(best) = resolve_best_path_match(
		&index,
		crate_name.as_deref(),
		&pkg_root,
		&base_query,
		is_local,
		include_private,
	) {
		matched_path = Some(best.path_string);
		matched_id = Some(best.item_id);
	} else if let Some((_ty_match, impl_id)) = resolve_impl_target(
		&index,
		&crate_data,
		crate_name.as_deref(),
		&pkg_root,
		&base_query,
		is_local,
		include_private,
	) {
		matched_path = Some(base_query.clone());
		matched_id = Some(impl_id);
	}
	
	// If no match and query starts with something other than crate name,
	// try replacing first segment with "crate" (unless --strict is set)
	if matched_id.is_none() && !strict {
		if let Some((first, rest)) = base_query.split_once("::") {
			if let Some(ref actual_crate) = crate_name {
				if first != actual_crate && first != "crate" {
					let crate_query = format!("crate::{}", rest);
					
					if let Some(best) = resolve_best_path_match(
						&index,
						crate_name.as_deref(),
						&pkg_root,
						&crate_query,
						is_local,
						include_private,
					) {
						matched_path = Some(best.path_string);
						matched_id = Some(best.item_id);
						eprintln!("Interpreted `{}` as `{}`", base_query, crate_query);
					} else if let Some((_ty_match, impl_id)) = resolve_impl_target(
						&index,
						&crate_data,
						crate_name.as_deref(),
						&pkg_root,
						&crate_query,
						is_local,
						include_private,
					) {
						matched_path = Some(crate_query.clone());
						matched_id = Some(impl_id);
						eprintln!("Interpreted `{}` as `{}`", base_query, crate_query);
					}
				}
			}
		}
	}

	let Some(matched_id) = matched_id else {
		// Generate smart suggestions when no match found
		let last_segment = base_query.rsplit("::").next().unwrap_or(&base_query);
		
		// Search by name
		let mut options = SearchOptions::new(last_segment);
		options.domains = SearchDomain::PATHS | SearchDomain::NAMES;
		options.include_private = true;
		let mut results = index.search(&options);
		results.retain(|r| is_local(r));
		
		// Prioritize results:
		// 1. Exact name match (highest priority)
		// 2. Path suffix match (e.g., user typed "module::Item", we match "crate::module::Item")
		// 3. Shortest paths (simpler is better)
		results.sort_by_key(|r| {
			let exact_name = r.raw_name != last_segment;
			let suffix_match = if base_query.contains("::") {
				!r.path_string.ends_with(&format!("::{}", base_query))
			} else {
				true // not a suffix search
			};
			let path_len = r.path_string.len();
			(exact_name, suffix_match, path_len)
		});
		
		results.truncate(5);
		let suggestions = results
			.iter()
			.map(|r| r.path_string.as_str())
			.collect::<Vec<_>>()
			.join("\n  - ");
		let suggestions = if suggestions.is_empty() {
			String::new()
		} else {
			format!("\nDid you mean:\n  - {suggestions}")
		};
		return Err(RipdocError::InvalidTarget(format!(
			"No path match found for `{base_query}` in `{}`.{suggestions}\n\nQuick recovery:\n  1) `ripdoc list {} --search \"{}\" --search-spec path --private`\n  2) Use the exact `crate::...` path from the listing.\n\nIf the item/module isn't present in rustdoc output (feature-gated or not in the module tree), include raw source via:\n  - `ripdoc skelebuild add-file <path>`\n  - `ripdoc skelebuild add-raw <path[:start[:end]]>`",
			pkg_root.display(),
			pkg_root.display(),
			last_segment,
		)));
	};
	let matched_path = matched_path.unwrap_or_else(|| base_query.clone());

	let (source_location, span_line_count) = crate_data
		.index
		.get(&matched_id)
		.and_then(|item| item.span.as_ref())
		.map(|span| {
			let mut display_path = span.filename.clone();
			if display_path.is_relative() {
				display_path = resolve_span_path(span);
			}
			let display_path = display_path
				.strip_prefix(&pkg_root)
				.map(|p| p.display().to_string())
				.unwrap_or_else(|_| display_path.display().to_string());
			let begin_line = span.begin.0;
			let end_line = span.end.0;
			let line_count = if begin_line > 0 && end_line >= begin_line {
				Some(end_line - begin_line + 1)
			} else {
				None
			};
			(Some(format!("{display_path}:{begin_line}")), line_count)
		})
		.unwrap_or((None, None));

	Ok(ValidatedTargetInfo {
		matched_path,
		source_location,
		span_line_count,
	})
}

/// Unescape backslash sequences in injection content (e.g., `\n` to newline).
pub fn unescape_inject_content(input: &str) -> String {
	let mut out = String::with_capacity(input.len());
	let mut chars = input.chars();
	while let Some(ch) = chars.next() {
		if ch != '\\' {
			out.push(ch);
			continue;
		}
		match chars.next() {
			Some('n') => out.push('\n'),
			Some('r') => out.push('\r'),
			Some('t') => out.push('\t'),
			Some('\\') => out.push('\\'),
			Some(other) => {
				out.push('\\');
				out.push(other);
			}
			None => out.push('\\'),
		}
	}
	out
}

/// Check if a stored target entry matches a user-provided search spec.
pub fn target_entry_matches_spec(stored_target: &str, spec: &str) -> bool {
	let spec = spec.trim();
	if spec.is_empty() {
		return false;
	}

	if stored_target == spec {
		return true;
	}

	// For path-based entries stored as "/abs/path/to/crate::item::path",
	// treat `spec` as an item-path matcher by default.
	let stored_item = stored_target
		.split_once("::")
		.map(|(_, item)| item)
		.unwrap_or(stored_target);

	stored_item == spec || stored_item.ends_with(&format!("::{spec}")) || stored_item.contains(spec)
}

/// Locate a target entry in the current state that matches the provided spec.
pub fn find_target_match(entries: &[SkeleEntry], spec: &str) -> Result<usize> {
	let mut matches: Vec<usize> = Vec::new();
	for (idx, entry) in entries.iter().enumerate() {
		let SkeleEntry::Target(t) = entry else {
			continue;
		};
		if target_entry_matches_spec(&t.path, spec) {
			matches.push(idx);
		}
	}

	match matches.as_slice() {
		[] => Err(RipdocError::InvalidTarget(format!(
			"No target matches '{spec}'. Use `ripdoc skelebuild status` to see entries.",
		))),
		[only] => Ok(*only),
		_ => Err(RipdocError::InvalidTarget(format!(
			"Ambiguous target match '{spec}': matches entries {matches:?}. Use a more specific `--after-target/--before-target` spec, or `inject --at <index>`.",
		))),
	}
}

/// Locate any entry (target or raw source) that matches the provided spec.
/// This is used for --after-target/--before-target which should work with any stable entry key.
pub fn find_entry_match(entries: &[SkeleEntry], spec: &str) -> Result<usize> {
	use super::state::SkeleEntry;
	
	let mut matches: Vec<usize> = Vec::new();
	let spec = spec.trim();
	
	// Normalize the spec for matching
	let normalized_spec = std::path::PathBuf::from(spec);
	let canonical_spec = normalized_spec.canonicalize().ok();
	let spec_str = spec.replace('\\', "/");
	
	for (idx, entry) in entries.iter().enumerate() {
		let is_match = match entry {
			SkeleEntry::Target(t) => {
				// Match target paths as before
				target_entry_matches_spec(&t.path, spec)
			}
			SkeleEntry::RawSource(r) => {
				// Try multiple matching strategies for raw source:
				// 1. Exact canonical key match
				let canonical_match = if let Some(ref key) = r.canonical_key {
					key == &spec_str || key == spec
				} else {
					false
				};
				
				// 2. Exact absolute path match
				let absolute_match = r.file.to_str() == Some(spec);
				
				// 3. Canonical path match (handles ./foo vs foo)
				let canon_path_match = if let Some(ref canon) = canonical_spec {
					r.file.canonicalize().ok().as_ref() == Some(canon)
				} else {
					false
				};
				
				canonical_match || absolute_match || canon_path_match
			}
			SkeleEntry::Injection(_) => false,
		};
		
		if is_match {
			matches.push(idx);
		}
	}

	match matches.as_slice() {
		[] => {
			// Provide helpful error with available keys
			let available_keys: Vec<String> = entries
				.iter()
				.enumerate()
				.filter_map(|(idx, entry)| match entry {
					SkeleEntry::Target(t) => Some(format!("  #{}: [target] {}", idx, t.path)),
					SkeleEntry::RawSource(r) => {
						if let Some(ref key) = r.canonical_key {
							Some(format!("  #{}: [raw] {}", idx, key))
						} else {
							Some(format!("  #{}: [raw] {}", idx, r.file.display()))
						}
					}
					SkeleEntry::Injection(_) => None,
				})
				.take(10)
				.collect();
			
			let available_str = if available_keys.is_empty() {
				String::new()
			} else {
				format!("\n\nAvailable entry keys (first 10):\n{}", available_keys.join("\n"))
			};
			
			Err(RipdocError::InvalidTarget(format!(
				"No entry matches '{spec}'.{}\n\nTip: Run `ripdoc skelebuild status` to see all entries.",
				available_str
			)))
		}
		[only] => Ok(*only),
		_ => Err(RipdocError::InvalidTarget(format!(
			"Ambiguous entry match '{spec}': matches entries {matches:?}. Use a more specific spec or `inject --at <index>`.",
		))),
	}
}
