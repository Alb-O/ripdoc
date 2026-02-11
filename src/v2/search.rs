#![cfg(feature = "v2-ts")]

use regex::Regex;

use crate::core_api::pattern::escape_regex_preserving_pipes;
use crate::core_api::{ListItem, SearchDomain, SearchOptions};

/// Filter list output using search options.
///
/// This slice currently supports `name` and `path` domains.
pub(crate) fn filter_list(items: Vec<ListItem>, options: &SearchOptions) -> Vec<ListItem> {
	let mut opts = options.clone();
	opts.ensure_domains();
	let query = opts.query.trim();
	if query.is_empty() {
		return Vec::new();
	}

	let domains = opts.domains;
	let case_sensitive = opts.case_sensitive;

	if query.contains('|') {
		filter_or_regex(items, query, domains, case_sensitive)
	} else {
		filter_simple(items, query, domains, case_sensitive)
	}
}

fn filter_simple(
	items: Vec<ListItem>,
	query: &str,
	domains: SearchDomain,
	case_sensitive: bool,
) -> Vec<ListItem> {
	let needle = if case_sensitive {
		query.to_string()
	} else {
		query.to_lowercase()
	};

	items
		.into_iter()
		.filter(|item| {
			let mut matched = false;
			if domains.contains(SearchDomain::NAMES) {
				let name = last_segment(&item.path);
				matched |= contains(name, &needle, case_sensitive);
			}
			if domains.contains(SearchDomain::PATHS) {
				matched |= contains(&item.path, &needle, case_sensitive);
			}
			matched
		})
		.collect()
}

fn filter_or_regex(
	items: Vec<ListItem>,
	pattern: &str,
	domains: SearchDomain,
	case_sensitive: bool,
) -> Vec<ListItem> {
	let escaped = escape_regex_preserving_pipes(pattern);
	let re = match if case_sensitive {
		Regex::new(&escaped)
	} else {
		Regex::new(&format!("(?i){escaped}"))
	} {
		Ok(re) => re,
		Err(_) => return filter_simple(items, pattern, domains, case_sensitive),
	};

	items
		.into_iter()
		.filter(|item| {
			let mut matched = false;
			if domains.contains(SearchDomain::NAMES) {
				matched |= re.is_match(last_segment(&item.path));
			}
			if domains.contains(SearchDomain::PATHS) {
				matched |= re.is_match(&item.path);
			}
			matched
		})
		.collect()
}

fn contains(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
	if needle.is_empty() {
		return false;
	}

	if case_sensitive {
		haystack.contains(needle)
	} else {
		haystack.to_lowercase().contains(needle)
	}
}

fn last_segment(path: &str) -> &str {
	path.rsplit("::").next().unwrap_or(path)
}
