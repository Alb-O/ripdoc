//! Hierarchical tree structure for organizing list output.

use std::collections::HashMap;

use super::search::{ListItem, SearchItemKind};

/// A hierarchical tree node representing a crate item and its children.
/// This provides a nested structure that reduces verbosity in JSON output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListTreeNode {
	/// Name of this item (not the full path).
	pub name: String,
	/// Kind classification for the item.
	pub kind: SearchItemKind,
	/// Source location for the item if available (format: "path/to/file.rs:line" or "path/to/file.rs:line:col").
	#[serde(skip_serializing_if = "Option::is_none", rename = "src")]
	pub source: Option<String>,
	/// Child items nested under this item.
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub children: Vec<ListTreeNode>,
}

impl ListTreeNode {
	/// Create a new tree node.
	pub fn new(name: String, kind: SearchItemKind, source: Option<String>) -> Self {
		Self {
			name,
			kind,
			source,
			children: Vec::new(),
		}
	}
}

/// Convert a flat list of items into a hierarchical tree structure.
///
/// Methods and associated types from trait implementations are excluded because their paths
/// contain trait names (e.g., `Type::Trait<T>::method`) which don't represent actual module
/// hierarchies and would create confusing intermediate nodes in the tree.
pub fn build_list_tree(items: &[ListItem]) -> Vec<ListTreeNode> {
	// Filter out methods and associated types from trait impls, which have paths that don't
	// represent real module hierarchies (e.g., pandoc::TrackChanges::Borrow<T>::borrow)
	let filtered_items: Vec<&ListItem> = items
		.iter()
		.filter(|item| {
			!matches!(
				item.kind,
				SearchItemKind::Method
					| SearchItemKind::TraitMethod
					| SearchItemKind::AssocType
					| SearchItemKind::AssocConst
			)
		})
		.collect();

	// Build a map from path to node
	let mut path_to_node: HashMap<String, ListTreeNode> = HashMap::new();
	let mut root_paths: Vec<String> = Vec::new();

	for item in &filtered_items {
		let segments: Vec<&str> = item.path.split("::").collect();

		// Build the tree from root to this item
		for i in 0..segments.len() {
			let current_path = segments[..=i].join("::");

			if !path_to_node.contains_key(&current_path) {
				let name = segments[i].to_string();
				let (kind, source) = if i == segments.len() - 1 {
					// This is the actual item
					(
						item.kind,
						item.source.as_ref().map(|s| s.to_compact_string()),
					)
				} else {
					// This is a parent path segment - infer it's a module or crate
					if i == 0 {
						(SearchItemKind::Crate, None)
					} else {
						(SearchItemKind::Module, None)
					}
				};

				let node = ListTreeNode::new(name, kind, source);
				path_to_node.insert(current_path.clone(), node);

				if i == 0 {
					root_paths.push(current_path);
				}
			}
		}
	}

	// Now build parent-child relationships
	let mut roots: HashMap<String, ListTreeNode> = HashMap::new();

	for item in &filtered_items {
		let segments: Vec<&str> = item.path.split("::").collect();

		for i in 0..segments.len() {
			let current_path = segments[..=i].join("::");

			if i == 0 {
				// Root level
				if !roots.contains_key(&current_path) {
					roots.insert(
						current_path.clone(),
						path_to_node.get(&current_path).unwrap().clone(),
					);
				}
			} else {
				// Child of parent
				let mut parent_node: Option<&mut ListTreeNode> = None;

				// Navigate to the parent
				for j in 0..i {
					let path_so_far = segments[..=j].join("::");
					if j == 0 {
						parent_node = roots.get_mut(&path_so_far);
					} else if let Some(node) = parent_node {
						parent_node = node.children.iter_mut().find(|c| {
							let expected_name = segments[j];
							c.name == expected_name
						});
					}
				}

				// Add child if not already present
				if let Some(parent) = parent_node {
					let child_name = segments[i];
					if !parent.children.iter().any(|c| c.name == child_name) {
						parent
							.children
							.push(path_to_node.get(&current_path).unwrap().clone());
					}
				}
			}
		}
	}

	// Sort children recursively
	fn sort_children(node: &mut ListTreeNode) {
		node.children.sort_by(|a, b| a.name.cmp(&b.name));
		for child in &mut node.children {
			sort_children(child);
		}
	}

	let mut result: Vec<ListTreeNode> = roots.into_values().collect();
	result.sort_by(|a, b| a.name.cmp(&b.name));
	for node in &mut result {
		sort_children(node);
	}

	result
}
