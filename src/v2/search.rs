#![cfg(feature = "v2-ts")]

use regex::Regex;

use crate::core_api::pattern::{escape_regex_preserving_pipes, strip_symbols_preserving_pipes};
use crate::core_api::{SearchDomain, SearchOptions};

use super::entry::V2Entry;

/// Filter entries using `SearchOptions`.
///
/// Supports `name`, `path`, `doc`, and `signature` domains.
pub(crate) fn filter_entries(entries: Vec<V2Entry>, options: &SearchOptions) -> Vec<V2Entry> {
	let mut opts = options.clone();
	opts.ensure_domains();
	let query = opts.query.trim();
	if query.is_empty() {
		return Vec::new();
	}

	if query.contains('|') {
		filter_or(entries, query, opts.domains, opts.case_sensitive)
	} else {
		filter_simple(entries, query, opts.domains, opts.case_sensitive)
	}
}

fn filter_simple(entries: Vec<V2Entry>, query: &str, domains: SearchDomain, case_sensitive: bool) -> Vec<V2Entry> {
	let normalized_query = if case_sensitive { query.to_string() } else { query.to_lowercase() };
	let stripped_query = strip_symbols_preserving_pipes(&normalized_query);

	entries
		.into_iter()
		.filter(|entry| {
			let mut matched = false;

			if domains.contains(SearchDomain::NAMES) {
				let name = if case_sensitive {
					entry.name().to_string()
				} else {
					entry.name().to_lowercase()
				};
				matched |= name.contains(&normalized_query);
			}

			if domains.contains(SearchDomain::PATHS) {
				let path = if case_sensitive { entry.path.clone() } else { entry.path.to_lowercase() };
				matched |= path.contains(&normalized_query);
			}

			if domains.contains(SearchDomain::DOCS)
				&& let Some(ref docs) = entry.docs
			{
				let docs = if case_sensitive { docs.clone() } else { docs.to_lowercase() };
				let stripped_docs = strip_symbols_preserving_pipes(&docs);
				matched |= stripped_docs.contains(&stripped_query);
			}

			if domains.contains(SearchDomain::SIGNATURES)
				&& let Some(ref signature) = entry.signature
			{
				let signature = if case_sensitive { signature.clone() } else { signature.to_lowercase() };
				let stripped_signature = strip_symbols_preserving_pipes(&signature);
				matched |= stripped_signature.contains(&stripped_query);
			}

			matched
		})
		.collect()
}

fn filter_or(entries: Vec<V2Entry>, pattern: &str, domains: SearchDomain, case_sensitive: bool) -> Vec<V2Entry> {
	let escaped = escape_regex_preserving_pipes(pattern);
	let regex = match if case_sensitive {
		Regex::new(&escaped)
	} else {
		Regex::new(&format!("(?i){escaped}"))
	} {
		Ok(re) => re,
		Err(_) => return filter_simple(entries, pattern, domains, case_sensitive),
	};

	let stripped_regex = if domains.intersects(SearchDomain::DOCS | SearchDomain::SIGNATURES) {
		let stripped_pattern = strip_symbols_preserving_pipes(pattern);
		let escaped_stripped = escape_regex_preserving_pipes(&stripped_pattern);
		if case_sensitive {
			Regex::new(&escaped_stripped).ok()
		} else {
			Regex::new(&format!("(?i){escaped_stripped}")).ok()
		}
	} else {
		None
	};

	entries
		.into_iter()
		.filter(|entry| {
			let mut matched = false;

			if domains.contains(SearchDomain::NAMES) {
				matched |= regex.is_match(entry.name());
			}

			if domains.contains(SearchDomain::PATHS) {
				matched |= regex.is_match(&entry.path);
			}

			if let Some(ref stripped_re) = stripped_regex {
				if domains.contains(SearchDomain::DOCS)
					&& let Some(ref docs) = entry.docs
				{
					let stripped_docs = strip_symbols_preserving_pipes(docs);
					matched |= stripped_re.is_match(&stripped_docs);
				}

				if domains.contains(SearchDomain::SIGNATURES)
					&& let Some(ref signature) = entry.signature
				{
					let stripped_signature = strip_symbols_preserving_pipes(signature);
					matched |= stripped_re.is_match(&stripped_signature);
				}
			}

			matched
		})
		.collect()
}
