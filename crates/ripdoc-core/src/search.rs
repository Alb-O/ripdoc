//! Internal search index implementation.
#![allow(clippy::missing_docs_in_private_items)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bitflags::bitflags;
use ripdoc_render::{
	RenderSelection, render_name, render_path, render_type, signatures as signature,
};
use rustdoc_types::{Crate, Id, Item, ItemEnum, Module, Struct, StructKind, Visibility};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
	/// Absolute path to the source file when available.
	pub path: String,
	/// One-indexed line number where the item starts.
	pub line: Option<usize>,
	/// One-indexed column number where the item starts.
	pub column: Option<usize>,
}

/// Lightweight record describing an item for list mode output.
#[derive(Debug, Clone, PartialEq, Eq)]
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
	fn clear_match_info(&mut self) {
		self.matched = SearchDomain::empty();
	}
}

/// Index of crate items prepared for search queries.
#[derive(Debug, Default, Clone)]
pub struct SearchIndex {
	entries: Vec<SearchResult>,
	id_to_entry: HashMap<Id, usize>,
}

impl SearchIndex {
	/// Construct a new index by traversing the provided crate.
	pub fn build(crate_data: &Crate, include_private: bool, source_root: Option<&Path>) -> Self {
		let mut builder = IndexBuilder::new(crate_data, include_private, source_root);
		builder.traverse();
		builder.finish()
	}

	/// Retrieve the immutable list of indexed entries.
	pub fn entries(&self) -> &[SearchResult] {
		&self.entries
	}

	/// Look up an indexed entry by ID.
	pub fn get(&self, id: &Id) -> Option<&SearchResult> {
		self.id_to_entry.get(id).map(|idx| &self.entries[*idx])
	}

	/// Prepare the index for a new search by clearing cached match metadata.
	pub fn reset_matches(&mut self) {
		for entry in &mut self.entries {
			entry.clear_match_info();
		}
	}

	/// Execute a query against the index and return matching results.
	pub fn search(&self, options: &SearchOptions) -> Vec<SearchResult> {
		let mut opts = options.clone();
		opts.ensure_domains();
		let trimmed = opts.query.trim();
		if trimmed.is_empty() {
			return Vec::new();
		}

		let normalized_query = if opts.case_sensitive {
			trimmed.to_string()
		} else {
			trimmed.to_lowercase()
		};

		// For signature and doc searches, also create a symbol-stripped version of the query
		let stripped_query = strip_symbols(&normalized_query);

		let mut results = Vec::new();
		for entry in &self.entries {
			let mut matched = SearchDomain::empty();
			if opts.domains.contains(SearchDomain::NAMES)
				&& contains(&entry.raw_name, &normalized_query, opts.case_sensitive)
			{
				matched |= SearchDomain::NAMES;
			}
			if opts.domains.contains(SearchDomain::DOCS)
				&& entry.docs.as_ref().is_some_and(|docs| {
					let stripped_docs = strip_symbols(docs);
					contains(&stripped_docs, &stripped_query, opts.case_sensitive)
				}) {
				matched |= SearchDomain::DOCS;
			}
			if opts.domains.contains(SearchDomain::PATHS)
				&& contains(&entry.path_string, &normalized_query, opts.case_sensitive)
			{
				matched |= SearchDomain::PATHS;
			}
			if opts.domains.contains(SearchDomain::SIGNATURES)
				&& entry.signature.as_ref().is_some_and(|sig| {
					let stripped_sig = strip_symbols(sig);
					contains(&stripped_sig, &stripped_query, opts.case_sensitive)
				}) {
				matched |= SearchDomain::SIGNATURES;
			}

			if !matched.is_empty() {
				let mut clone = entry.clone();
				clone.matched = matched;
				results.push(clone);
			}
		}

		results
	}
}

#[derive(Clone)]
struct PathStackEntry {
	id: Option<Id>,
	segment: SearchPathSegment,
}

struct ImplContext {
	pushed: Vec<PathStackEntry>,
	impl_id: Id,
}

struct IndexBuilder<'a> {
	crate_data: &'a Crate,
	include_private: bool,
	source_root: Option<PathBuf>,
	source_prefix: Option<String>,
	stack: Vec<PathStackEntry>,
	entries: Vec<SearchResult>,
	visited: HashSet<Id>,
}

impl<'a> IndexBuilder<'a> {
	fn new(crate_data: &'a Crate, include_private: bool, source_root: Option<&Path>) -> Self {
		let crate_name = crate_data
			.index
			.get(&crate_data.root)
			.and_then(|root| root.name.clone());
		let root_name = source_root
			.and_then(Path::file_name)
			.and_then(|os| os.to_str())
			.map(str::to_string);
		let source_prefix = match (root_name, crate_name.clone()) {
			(Some(root), Some(crate_name)) => {
				if root.starts_with(&format!("{crate_name}-")) {
					Some(root)
				} else {
					Some(crate_name)
				}
			}
			(None, Some(crate_name)) => Some(crate_name),
			(Some(root), None) => Some(root),
			(None, None) => None,
		};
		Self {
			crate_data,
			include_private,
			source_root: source_root.map(PathBuf::from),
			source_prefix,
			stack: Vec::new(),
			entries: Vec::new(),
			visited: HashSet::new(),
		}
	}

	fn traverse(&mut self) {
		if let Some(root) = self.crate_data.index.get(&self.crate_data.root) {
			self.visit_root(root);
		}
	}

	fn finish(self) -> SearchIndex {
		let entries: Vec<SearchResult> = self.entries;
		let mut id_to_entry = HashMap::with_capacity(entries.len());
		for (idx, entry) in entries.iter().enumerate() {
			id_to_entry.insert(entry.item_id, idx);
		}
		SearchIndex {
			entries,
			id_to_entry,
		}
	}

	fn visit_root(&mut self, item: &Item) {
		if let ItemEnum::Module(module) = &item.inner {
			let segment = self.make_segment(item, SearchItemKind::Crate, Some("crate"));
			self.record_item(item, SearchItemKind::Crate, &segment, true, &[]);
			self.stack.push(PathStackEntry {
				id: Some(item.id),
				segment,
			});
			self.visit_module_items(module);
			self.stack.pop();
		}
	}

	fn visit_item(&mut self, item_id: &Id) {
		if !self.visited.insert(*item_id) {
			return;
		}
		let Some(item) = self.crate_data.index.get(item_id) else {
			return;
		};
		match &item.inner {
			ItemEnum::Module(module) => self.visit_module(item, module),
			ItemEnum::Struct(struct_) => self.visit_struct(item, struct_),
			ItemEnum::Enum(enum_) => self.visit_enum(item, enum_),
			ItemEnum::Union(union_) => self.visit_union(item, union_),
			ItemEnum::Trait(trait_) => self.visit_trait(item, trait_),
			ItemEnum::Function(_) => self.visit_function(item),
			ItemEnum::TypeAlias(_) => self.record_simple(item, SearchItemKind::TypeAlias),
			ItemEnum::Constant { .. } => self.record_simple(item, SearchItemKind::Constant),
			ItemEnum::Static(_) => self.record_simple(item, SearchItemKind::Static),
			ItemEnum::Macro(_) => self.record_simple(item, SearchItemKind::Macro),
			ItemEnum::ProcMacro(_) => self.record_simple(item, SearchItemKind::ProcMacro),
			ItemEnum::TraitAlias(_) => self.record_simple(item, SearchItemKind::TraitAlias),
			ItemEnum::Use(_) => self.record_simple(item, SearchItemKind::Use),
			ItemEnum::Primitive(_) => self.record_simple(item, SearchItemKind::Primitive),
			ItemEnum::Variant(variant) => self.visit_variant(item, variant),
			ItemEnum::StructField(_) => self.record_simple(item, SearchItemKind::Field),
			ItemEnum::AssocConst { .. } => self.record_simple(item, SearchItemKind::AssocConst),
			ItemEnum::AssocType { .. } => self.record_simple(item, SearchItemKind::AssocType),
			ItemEnum::Impl(impl_) => self.visit_impl(item, impl_),
			ItemEnum::ExternCrate { .. } | ItemEnum::ExternType => {}
		}
	}

	fn visit_module(&mut self, item: &Item, module: &Module) {
		let segment = self.make_segment(item, SearchItemKind::Module, None);
		let _ = self.record_item(item, SearchItemKind::Module, &segment, module.is_crate, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		self.visit_module_items(module);
		self.stack.pop();
	}

	fn visit_module_items(&mut self, module: &Module) {
		for child in &module.items {
			self.visit_item(child);
		}
	}

	fn visit_struct(&mut self, item: &Item, struct_: &Struct) {
		let segment = self.make_segment(item, SearchItemKind::Struct, None);
		let include_children = self.record_item(item, SearchItemKind::Struct, &segment, false, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		if include_children {
			match &struct_.kind {
				StructKind::Unit => {}
				StructKind::Tuple(fields) => {
					for field_id in fields.iter().flatten() {
						self.visit_item(field_id);
					}
				}
				StructKind::Plain { fields, .. } => {
					for field in fields {
						self.visit_item(field);
					}
				}
			}
		}
		for impl_id in &struct_.impls {
			self.visit_item(impl_id);
		}
		self.stack.pop();
	}

	fn visit_union(&mut self, item: &Item, union_: &rustdoc_types::Union) {
		let segment = self.make_segment(item, SearchItemKind::Union, None);
		let include_children = self.record_item(item, SearchItemKind::Union, &segment, false, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		if include_children {
			for field in &union_.fields {
				self.visit_item(field);
			}
		}
		for impl_id in &union_.impls {
			self.visit_item(impl_id);
		}
		self.stack.pop();
	}

	fn visit_enum(&mut self, item: &Item, enum_: &rustdoc_types::Enum) {
		let segment = self.make_segment(item, SearchItemKind::Enum, None);
		let include_children = self.record_item(item, SearchItemKind::Enum, &segment, false, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		if include_children {
			for variant_id in &enum_.variants {
				self.visit_item(variant_id);
			}
		}
		for impl_id in &enum_.impls {
			self.visit_item(impl_id);
		}
		self.stack.pop();
	}

	fn visit_variant(&mut self, item: &Item, variant: &rustdoc_types::Variant) {
		let segment = self.make_segment(item, SearchItemKind::EnumVariant, None);
		let include_children =
			self.record_item(item, SearchItemKind::EnumVariant, &segment, false, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		if include_children {
			match &variant.kind {
				rustdoc_types::VariantKind::Plain => {}
				rustdoc_types::VariantKind::Tuple(fields) => {
					for field_id in fields.iter().flatten() {
						self.visit_item(field_id);
					}
				}
				rustdoc_types::VariantKind::Struct { fields, .. } => {
					for field_id in fields {
						self.visit_item(field_id);
					}
				}
			}
		}
		self.stack.pop();
	}

	fn visit_trait(&mut self, item: &Item, trait_: &rustdoc_types::Trait) {
		let segment = self.make_segment(item, SearchItemKind::Trait, None);
		let include_children = self.record_item(item, SearchItemKind::Trait, &segment, false, &[]);
		self.stack.push(PathStackEntry {
			id: Some(item.id),
			segment,
		});
		if include_children {
			for assoc_id in &trait_.items {
				if let Some(assoc) = self.crate_data.index.get(assoc_id) {
					match &assoc.inner {
						ItemEnum::Function(_) => {
							self.record_trait_member(assoc, SearchItemKind::TraitMethod)
						}
						ItemEnum::AssocConst { .. } => {
							self.record_trait_member(assoc, SearchItemKind::AssocConst)
						}
						ItemEnum::AssocType { .. } => {
							self.record_trait_member(assoc, SearchItemKind::AssocType)
						}
						_ => self.visit_item(assoc_id),
					}
				}
			}
		}
		for impl_id in &trait_.implementations {
			self.visit_item(impl_id);
		}
		self.stack.pop();
	}

	fn visit_function(&mut self, item: &Item) {
		self.record_simple(item, SearchItemKind::Function);
	}

	fn record_trait_member(&mut self, item: &Item, kind: SearchItemKind) {
		let segment = self.make_segment(item, kind, None);
		self.record_item(item, kind, &segment, false, &[]);
	}

	fn visit_impl(&mut self, impl_item: &Item, impl_: &rustdoc_types::Impl) {
		if impl_.is_synthetic {
			return;
		}

		let ctx = self.enter_impl_context(impl_item, impl_);
		for member_id in &impl_.items {
			if let Some(member) = self.crate_data.index.get(member_id) {
				match &member.inner {
					ItemEnum::Function(_) => {
						self.record_impl_member(member, SearchItemKind::Method, &ctx)
					}
					ItemEnum::AssocConst { .. } => {
						self.record_impl_member(member, SearchItemKind::AssocConst, &ctx)
					}
					ItemEnum::AssocType { .. } => {
						self.record_impl_member(member, SearchItemKind::AssocType, &ctx)
					}
					ItemEnum::TypeAlias(_) => {
						self.record_impl_member(member, SearchItemKind::TypeAlias, &ctx)
					}
					ItemEnum::Constant { .. } => {
						self.record_impl_member(member, SearchItemKind::Constant, &ctx)
					}
					_ => self.visit_item(member_id),
				}
			}
		}

		self.exit_impl_context(ctx);
	}

	fn record_impl_member(&mut self, item: &Item, kind: SearchItemKind, ctx: &ImplContext) {
		let segment = self.make_segment(item, kind, None);
		self.record_item(item, kind, &segment, false, &[ctx.impl_id]);
	}

	fn enter_impl_context(&mut self, impl_item: &Item, impl_: &rustdoc_types::Impl) -> ImplContext {
		let mut ctx = ImplContext {
			pushed: Vec::new(),
			impl_id: impl_item.id,
		};

		if let Some(target_entry) = self.impl_target_entry(&impl_.for_) {
			let has_target = target_entry
				.id
				.and_then(|id| {
					self.stack
						.iter()
						.find(|entry| entry.id == Some(id))
						.map(|_| ())
				})
				.is_some();
			if !has_target {
				self.stack.push(target_entry.clone());
				ctx.pushed.push(target_entry);
			}
		}

		if let Some(trait_entry) = self.impl_trait_entry(&impl_.trait_) {
			self.stack.push(trait_entry.clone());
			ctx.pushed.push(trait_entry);
		}

		ctx
	}

	fn exit_impl_context(&mut self, ctx: ImplContext) {
		for _ in ctx.pushed {
			self.stack.pop();
		}
	}

	fn impl_trait_entry(&self, trait_path: &Option<rustdoc_types::Path>) -> Option<PathStackEntry> {
		trait_path.as_ref().map(|path| {
			let display = render_path(path);
			let (id, kind, is_public) =
				if let Some(trait_item) = self.crate_data.index.get(&path.id) {
					(
						Some(path.id),
						SearchItemKind::Trait,
						matches!(
							trait_item.visibility,
							Visibility::Public | Visibility::Default
						),
					)
				} else {
					(None, SearchItemKind::Trait, true)
				};
			PathStackEntry {
				id,
				segment: SearchPathSegment {
					name: display.clone(),
					display_name: display,
					kind,
					is_public,
				},
			}
		})
	}

	fn impl_target_entry(&self, ty: &rustdoc_types::Type) -> Option<PathStackEntry> {
		match ty {
			rustdoc_types::Type::ResolvedPath(path) => {
				let name = render_type(ty);
				if let Some(item) = self.crate_data.index.get(&path.id) {
					let kind = self
						.kind_from_item(item)
						.unwrap_or(SearchItemKind::ImplTarget);
					let segment = self.make_segment(item, kind, None);
					Some(PathStackEntry {
						id: Some(item.id),
						segment: SearchPathSegment {
							name: name.clone(),
							display_name: name,
							kind: SearchItemKind::ImplTarget,
							is_public: segment.is_public,
						},
					})
				} else {
					Some(PathStackEntry {
						id: None,
						segment: SearchPathSegment {
							name: name.clone(),
							display_name: name,
							kind: SearchItemKind::ImplTarget,
							is_public: true,
						},
					})
				}
			}
			_ => {
				let name = render_type(ty);
				Some(PathStackEntry {
					id: None,
					segment: SearchPathSegment {
						name: name.clone(),
						display_name: name,
						kind: SearchItemKind::ImplTarget,
						is_public: true,
					},
				})
			}
		}
	}

	fn kind_from_item(&self, item: &Item) -> Option<SearchItemKind> {
		match item.inner {
			ItemEnum::Module(_) => Some(SearchItemKind::Module),
			ItemEnum::Struct(_) => Some(SearchItemKind::Struct),
			ItemEnum::Enum(_) => Some(SearchItemKind::Enum),
			ItemEnum::Union(_) => Some(SearchItemKind::Union),
			ItemEnum::Trait(_) => Some(SearchItemKind::Trait),
			ItemEnum::TraitAlias(_) => Some(SearchItemKind::TraitAlias),
			ItemEnum::Function(_) => Some(SearchItemKind::Function),
			ItemEnum::TypeAlias(_) => Some(SearchItemKind::TypeAlias),
			ItemEnum::Constant { .. } => Some(SearchItemKind::Constant),
			ItemEnum::Static(_) => Some(SearchItemKind::Static),
			ItemEnum::Macro(_) => Some(SearchItemKind::Macro),
			ItemEnum::ProcMacro(_) => Some(SearchItemKind::ProcMacro),
			ItemEnum::Primitive(_) => Some(SearchItemKind::Primitive),
			ItemEnum::StructField(_) => Some(SearchItemKind::Field),
			ItemEnum::Variant(_) => Some(SearchItemKind::EnumVariant),
			_ => None,
		}
	}

	fn record_simple(&mut self, item: &Item, kind: SearchItemKind) {
		let segment = self.make_segment(item, kind, None);
		self.record_item(item, kind, &segment, false, &[]);
	}

	fn make_segment(
		&self,
		item: &Item,
		kind: SearchItemKind,
		fallback: Option<&str>,
	) -> SearchPathSegment {
		let raw_name = item
			.name
			.as_deref()
			.map(ToOwned::to_owned)
			.or_else(|| fallback.map(ToOwned::to_owned))
			.unwrap_or_else(|| "?".to_string());
		let display_name = if item.name.is_some() {
			render_name(item)
		} else {
			raw_name.clone()
		};
		SearchPathSegment {
			name: raw_name,
			display_name,
			kind,
			is_public: matches!(item.visibility, Visibility::Public | Visibility::Default),
		}
	}

	fn record_item(
		&mut self,
		item: &Item,
		kind: SearchItemKind,
		segment: &SearchPathSegment,
		always_include: bool,
		extra_ancestors: &[Id],
	) -> bool {
		if !always_include && !self.should_include(item) {
			return false;
		}

		let mut path: Vec<SearchPathSegment> = self
			.stack
			.iter()
			.map(|entry| entry.segment.clone())
			.collect();
		path.push(segment.clone());

		let mut ancestors: Vec<Id> = self.stack.iter().filter_map(|entry| entry.id).collect();
		ancestors.extend(extra_ancestors.iter().copied());

		let path_string = join_path(&path);
		let source = self.resolve_source(item);
		let signature = self.signature_for(item, kind);
		let result = SearchResult {
			item_id: item.id,
			kind,
			path,
			path_string,
			raw_name: segment.name.clone(),
			display_name: segment.display_name.clone(),
			docs: item.docs.clone(),
			signature,
			source,
			ancestors,
			matched: SearchDomain::empty(),
		};

		self.entries.push(result);
		true
	}

	fn resolve_source(&self, item: &Item) -> Option<SourceLocation> {
		let span = item.span.as_ref()?;
		let mut path = span.filename.clone();

		if let Some(root) = &self.source_root
			&& path.is_relative()
		{
			path = root.join(path);
		}

		let absolute_path = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => path,
		};

		let display_path = self
			.source_root
			.as_deref()
			.and_then(|root| {
				let rel = absolute_path.strip_prefix(root).ok()?;
				let mut buf = PathBuf::new();
				if let Some(name) = self.source_prefix.as_deref() {
					buf.push(name);
				}
				buf.push(rel);
				Some(buf)
			})
			.or_else(|| {
				self.find_manifest_root(&absolute_path)
					.and_then(|(manifest_root, root_name)| {
						let rel = absolute_path.strip_prefix(&manifest_root).ok()?;
						let mut buf = PathBuf::from(root_name);
						buf.push(rel);
						Some(buf)
					})
			})
			.unwrap_or_else(|| absolute_path.clone());

		Some(SourceLocation {
			path: display_path.to_string_lossy().into_owned(),
			line: Some(span.begin.0),
			column: None,
		})
	}

	fn find_manifest_root(&self, path: &Path) -> Option<(PathBuf, String)> {
		for ancestor in path.ancestors() {
			let manifest = ancestor.join("Cargo.toml");
			if manifest.exists() {
				let name = ancestor
					.file_name()
					.and_then(|os| os.to_str())
					.map(str::to_string)?;
				return Some((ancestor.to_path_buf(), name));
			}
		}
		None
	}

	fn signature_for(&self, item: &Item, kind: SearchItemKind) -> Option<String> {
		match (&item.inner, kind) {
			(ItemEnum::Function(_), SearchItemKind::Function)
			| (ItemEnum::Function(_), SearchItemKind::Method)
			| (ItemEnum::Function(_), SearchItemKind::TraitMethod) => {
				Some(signature::function_signature(item))
			}
			(ItemEnum::StructField(_), SearchItemKind::Field) => {
				Some(signature::field_signature(item))
			}
			(ItemEnum::Struct(_), SearchItemKind::Struct) => {
				Some(signature::struct_signature(item))
			}
			(ItemEnum::Union(_), SearchItemKind::Union) => Some(signature::union_signature(item)),
			(ItemEnum::Enum(_), SearchItemKind::Enum) => Some(signature::enum_signature(item)),
			(ItemEnum::Trait(_), SearchItemKind::Trait) => Some(signature::trait_signature(item)),
			(ItemEnum::TraitAlias(_), SearchItemKind::TraitAlias) => {
				Some(signature::trait_alias_signature(item))
			}
			(ItemEnum::TypeAlias(_), SearchItemKind::TypeAlias) => {
				Some(signature::type_alias_signature(item))
			}
			(ItemEnum::Constant { .. }, SearchItemKind::Constant) => {
				Some(signature::constant_signature(item))
			}
			(ItemEnum::Static(_), SearchItemKind::Static) => {
				Some(signature::static_signature(item))
			}
			(ItemEnum::AssocConst { .. }, SearchItemKind::AssocConst) => {
				Some(signature::assoc_const_signature(item))
			}
			(ItemEnum::AssocType { .. }, SearchItemKind::AssocType) => {
				Some(signature::assoc_type_signature(item))
			}
			(ItemEnum::Macro(_), SearchItemKind::Macro) => Some(signature::macro_signature(item)),
			(ItemEnum::ProcMacro(_), SearchItemKind::ProcMacro) => {
				Some(signature::proc_macro_signature(item))
			}
			(ItemEnum::Use(_), SearchItemKind::Use) => Some(signature::use_signature(item)),
			(ItemEnum::Primitive(_), SearchItemKind::Primitive) => {
				Some(signature::primitive_signature(item))
			}
			(ItemEnum::Module(_), SearchItemKind::Module) => {
				Some(signature::module_signature(item))
			}
			(ItemEnum::Module(_), SearchItemKind::Crate) => Some(render_name(item)),
			(ItemEnum::Variant(variant), SearchItemKind::EnumVariant) => {
				let field_lookup = |field_id: &Id| {
					self.crate_data.index.get(field_id).and_then(|field_item| {
						match &field_item.inner {
							ItemEnum::StructField(ty) => {
								let name = field_item.name.as_deref().unwrap_or("_");
								if matches!(variant.kind, rustdoc_types::VariantKind::Struct { .. })
								{
									Some(format!("{}: {}", name, render_type(ty)))
								} else {
									Some(render_type(ty))
								}
							}
							_ => None,
						}
					})
				};
				Some(signature::variant_signature(item, variant, field_lookup))
			}
			_ => None,
		}
	}

	fn should_include(&self, item: &Item) -> bool {
		if self.include_private {
			return true;
		}
		matches!(item.visibility, Visibility::Public | Visibility::Default)
	}
}

fn join_path(path: &[SearchPathSegment]) -> String {
	let mut out = String::new();
	for (idx, segment) in path.iter().enumerate() {
		if idx > 0 {
			out.push_str("::");
		}
		out.push_str(&segment.name);
	}
	out
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

/// Strip Rust syntax symbols from text, keeping only alphanumeric characters, underscores, and whitespace.
/// This is used when searching signatures to avoid matching against irrelevant syntax characters.
fn strip_symbols(text: &str) -> String {
	text.chars()
		.filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_')
		.collect()
}

/// Build a renderer selection set covering matches, their ancestors, and optionally their children.
pub fn build_render_selection(
	index: &SearchIndex,
	results: &[SearchResult],
	expand_containers: bool,
) -> RenderSelection {
	let mut matches = HashSet::new();
	let mut context = HashSet::new();
	let mut expanded = HashSet::new();
	for result in results {
		matches.insert(result.item_id);
		context.insert(result.item_id);
		context.extend(result.ancestors.iter().copied());
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

	RenderSelection::new(matches, context, expanded)
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
