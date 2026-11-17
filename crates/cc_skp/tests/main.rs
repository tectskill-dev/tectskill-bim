//! The crate's integration tests as one binary (one link instead of one per
//! file). Each file in this folder is a module here; `every_test_file_is_a_module`
//! fails when one is left out.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod arc_curves_test;
mod common;
mod create_hardening_test;
mod create_reference_test;
mod create_roundtrip_test;
mod create_test;
mod definition_is_group_test;
mod edit_test;
mod face_instance_hidden_test;
mod geometry_test;
mod hardening_test;
mod inflate_test;
mod instance_scale_term_test;
mod instanced_scene_test;
mod layer_hidden_test;
mod legacy_anchored_texts_test;
mod legacy_dimension_geometry_test;
mod legacy_dimension_paths_test;
mod legacy_empty_model_test;
mod legacy_python_extras_test;
mod legacy_scenes_test;
mod legacy_single_material_test;
mod legacy_textured_colorize_test;
mod legacy_v20_filler_test;
mod legacy_v20_test;
mod legacy_v3_format_test;
mod legacy_v4_v5_v2019_test;
mod legacy_v6_attribute_trailer_test;
mod loose_edge_curve_sets_test;
mod lowlevel_test;
mod meta_units_test;
mod model_test;
mod native_compatibility_test;
mod observability_test;
mod pages_dimensions_test;
mod parser_test;
mod scene_test;
mod scene_without_camera_test;
mod texture_test;
mod triangulator_test;
mod vff_layer_hidden_test;
mod xml_entities_test;
mod zip_entry_size_cap_test;

/// Every `.rs` file of `tests/` is declared above, so none is skipped.
#[test]
fn every_test_file_is_a_module() {
    let main = include_str!("main.rs");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let Some(stem) =
            path.file_stem().and_then(|s| s.to_str()).filter(|_| path.extension().is_some_and(|e| e == "rs"))
        else {
            continue;
        };
        assert!(
            stem == "main" || main.contains(&format!("\nmod {stem};")),
            "tests/{stem}.rs is not a module of tests/main.rs"
        );
    }
}
