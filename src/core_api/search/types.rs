use bitflags::bitflags;
use rustdoc_types::Id;

bitflags! {
	/// Domains that a search query can operate over.
	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
	pub struct SearchDomain: u32 {
		/// Match against item names.
		const NAMES = 1 << 0;
		/// Match against documentation strings.
		const DOCS = 1 << 1;
		/// Match against canonical module paths.
		const PATHS = 1 << 2;
		/// Match against rendered item signatures.
		const SIGNATURES = 1 << 3;
	}
}

impl Default for SearchDomain {
	fn default() -> Self {
		Self::NAMES | Self::DOCS | Self::SIGNATURES
	}
}

/// Options that control how a crate search should be performed.
#[derive(Debug, Clone)]
pub struct SearchOptions {
	/// Raw user query to evaluate.
	pub query: String,
	/// Domains to search across; defaults to [`SearchDomain::default`].
	pub domains: SearchDomain,
	/// Whether matching should respect letter casing.
	pub case_sensitive: bool,
	/// Whether to include private or crate-private items.
	pub include_private: bool,
	/// Whether matched container items should expand to include their children.
	pub expand_containers: bool,
}

impl SearchOptions {
	/// Create a new options struct with the provided query string.
	pub fn new(query: impl Into<String>) -> Self {
		Self {
			query: query.into(),
			domains: SearchDomain::default(),
			case_sensitive: false,
			include_private: false,
			expand_containers: true,
		}
	}

	/// Ensure the options have at least one domain selected.
	pub fn ensure_domains(&mut self) {
		if self.domains.is_empty() {
			self.domains = SearchDomain::default();
		}
	}
}

/// Classified kind associated with a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchItemKind {
	/// Synthetic crate root module.
	Crate,
	/// Regular module.
	Module,
	/// Struct definition.
	Struct,
	/// Union definition.
	Union,
	/// Enum definition.
	Enum,
	/// Variant within an enum.
	EnumVariant,
	/// Named or positional field within a struct or union.
	Field,
	/// Trait definition.
	Trait,
	/// Trait alias definition.
	TraitAlias,
	/// Free function.
	Function,
	/// Method inside an impl block.
	Method,
	/// Trait method declaration.
	TraitMethod,
	/// Associated constant.
	AssocConst,
	/// Associated type.
	AssocType,
	/// Top-level constant.
	Constant,
	/// Static item.
	Static,
	/// Type alias.
	TypeAlias,
	/// `use` declaration.
	Use,
	/// Macro_rules! definition.
	Macro,
	/// Procedural macro entrypoint.
	ProcMacro,
	/// Primitive type description.
	Primitive,
	/// Synthetic segment representing an impl target.
	ImplTarget,
}

impl SearchItemKind {
	/// Human-friendly label describing the item kind.
	pub fn label(self) -> &'static str {
		match self {
			Self::Crate => "crate",
			Self::Module => "module",
			Self::Struct => "struct",
			Self::Union => "union",
			Self::Enum => "enum",
			Self::EnumVariant => "enum variant",
			Self::Field => "field",
			Self::Trait => "trait",
			Self::TraitAlias => "trait alias",
			Self::Function => "function",
			Self::Method => "method",
			Self::TraitMethod => "trait method",
			Self::AssocConst => "assoc const",
			Self::AssocType => "assoc type",
			Self::Constant => "constant",
			Self::Static => "static",
			Self::TypeAlias => "type alias",
			Self::Use => "use",
			Self::Macro => "macro",
			Self::ProcMacro => "proc macro",
			Self::Primitive => "primitive",
			Self::ImplTarget => "impl target",
		}
	}
}

/// Component in a canonical path leading to an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPathSegment {
	/// Raw identifier for the segment without keyword escaping.
	pub name: String,
	/// Display name used when rendering the path.
	pub display_name: String,
	/// Classification of the segment.
	pub kind: SearchItemKind,
	/// Whether the segment corresponds to a publicly visible item.
	pub is_public: bool,
}

/// Aggregated search response containing matches and rendered output.
#[derive(Debug, Clone)]
pub struct SearchResponse {
	/// Matched records returned by the index.
	pub results: Vec<SearchResult>,
	/// Rendered skeleton filtered to only include matched items.
	pub rendered: String,
}

/// Source location associated with an item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceLocation {
	/// Absolute path to the source file when available.
	pub path: String,
	/// One-indexed line number where the item starts.
	pub line: Option<usize>,
	/// One-indexed column number where the item starts.
	pub column: Option<usize>,
}

impl SourceLocation {
	/// Format the source location as a compact string (e.g., "path/to/file.rs:42" or "path/to/file.rs:42:10").
	pub fn to_compact_string(&self) -> String {
		match (self.line, self.column) {
			(Some(line), Some(col)) => format!("{}:{}:{}", self.path, line, col),
			(Some(line), None) => format!("{}:{}", self.path, line),
			_ => self.path.clone(),
		}
	}
}

/// Lightweight record describing an item for list mode output.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListItem {
	/// Kind classification for the item.
	pub kind: SearchItemKind,
	/// Canonical path rendered as a `::` separated string.
	pub path: String,
	/// Source location for the item if available.
	pub source: Option<SourceLocation>,
}

/// Result of performing a query against a crate index.
#[derive(Debug, Clone)]
pub struct SearchResult {
	/// Identifier of the matching item.
	pub item_id: Id,
	/// Kind of result item.
	pub kind: SearchItemKind,
	/// Canonical path segments to reach the item.
	pub path: Vec<SearchPathSegment>,
	/// Canonical path rendered as a `::` separated string.
	pub path_string: String,
	/// Raw identifier of the item.
	pub raw_name: String,
	/// Display name formatted for rendering.
	pub display_name: String,
	/// Documentation snippet if available.
	pub docs: Option<String>,
	/// Rendered signature used for matching and display.
	pub signature: Option<String>,
	/// Source location for the item if available.
	pub source: Option<SourceLocation>,
	/// Ancestor chain of items that must be rendered for context.
	pub ancestors: Vec<Id>,
	/// Domains that produced a match (empty when stored in the index).
	pub matched: SearchDomain,
}

impl SearchResult {
	/// Reset match metadata so the record can be reused for a new query.
	pub(crate) fn clear_match_info(&mut self) {
		self.matched = SearchDomain::empty();
	}
}
