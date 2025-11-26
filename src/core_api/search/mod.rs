//! Internal search index implementation.
#![allow(clippy::missing_docs_in_private_items)]

mod index;
mod selection;
mod types;

pub use index::SearchIndex;
pub use selection::{build_render_selection, describe_domains};
pub use types::{
	ListItem, SearchDomain, SearchItemKind, SearchOptions, SearchPathSegment, SearchResponse,
	SearchResult, SourceLocation,
};

#[cfg(test)]
mod tests;
