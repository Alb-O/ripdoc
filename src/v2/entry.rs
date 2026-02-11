#![cfg(feature = "v2-ts")]

use crate::core_api::{ListItem, SearchItemKind, SourceLocation};

#[derive(Debug, Clone)]
pub(crate) struct V2Entry {
	pub(crate) kind: SearchItemKind,
	pub(crate) path: String,
	pub(crate) source: Option<SourceLocation>,
	pub(crate) docs: Option<String>,
	pub(crate) signature: Option<String>,
	pub(crate) public_api: bool,
}

impl V2Entry {
	pub(crate) fn name(&self) -> &str {
		self.path.rsplit("::").next().unwrap_or(&self.path)
	}

	pub(crate) fn to_list_item(&self) -> ListItem {
		ListItem {
			kind: self.kind,
			path: self.path.clone(),
			source: self.source.clone(),
		}
	}
}
