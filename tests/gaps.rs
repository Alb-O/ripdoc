//! Gap marker integration tests to ensure search-mode rendering does not duplicate markers.
mod utils;

use std::collections::HashSet;

use ripdoc::core_api::Renderer;
use ripdoc::render::RenderSelection;
use ripdoc::render::utils::GAP_MARKER;
use ripdoc::{RenderFormat, Ripdoc};
use rustdoc_types::ItemEnum;
use utils::*;

#[test]
fn use_glob_emits_single_gap_marker() {
	let source = r#"
        pub mod inner {
            /// Matched symbol.
            pub fn target() {}

            /// Unmatched symbol that should be hidden.
            pub fn other() {}
        }

        pub use inner::*;
    "#;

	let (_temp_dir, target) = create_test_crate(source, false);
	let ripdoc = Ripdoc::new().with_offline(true).with_silent(true);
	let crate_data = ripdoc
		.inspect(&target, false, false, Vec::new(), true)
		.unwrap();

	let mut target_id = None;
	let mut use_id = None;
	let mut inner_id = None;
	for (id, item) in &crate_data.index {
		match &item.inner {
			ItemEnum::Function(_) if item.name.as_deref() == Some("target") => {
				target_id = Some(*id)
			}
			ItemEnum::Use(_) => use_id = Some(*id),
			ItemEnum::Module(_) if item.name.as_deref() == Some("inner") => inner_id = Some(*id),
			_ => {}
		}
	}

	let target_id = target_id.expect("expected target function id");
	let use_id = use_id.expect("expected use id");
	let inner_id = inner_id.expect("expected inner module id");

	let matches = HashSet::from([target_id]);
	let context = HashSet::from([crate_data.root, inner_id, target_id, use_id]);
	let expanded = HashSet::new();
	let selection = RenderSelection::new(matches, context, expanded);

	let renderer = Renderer::default()
		.with_private_items(true)
		.with_format(RenderFormat::Rust)
		.with_selection(selection);

	let rendered = renderer.render(&crate_data).unwrap();
	let gap_count = rendered.matches(GAP_MARKER).count();
	assert_eq!(
		gap_count, 1,
		"expected a single gap marker in output but found {gap_count}:\n{rendered}"
	);
	assert!(
		rendered.contains("pub fn target"),
		"rendered output should include the matched item:\n{rendered}"
	);
}
