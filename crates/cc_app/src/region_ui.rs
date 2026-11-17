//! Filled and masking regions in the app: *Annotate → Filled Region* and
//! *Masking Region* open the sketch mode in the active view (a plan,
//! ceiling plan, elevation, section, legend or drafting view; curves in the
//! view's drawing coordinates), and *Finish* runs `filled_region_create` or
//! `masking_region_create` with the curves. *Edit Boundary* on a selected
//! region redraws it (`sketch_set`). The properties palette shows the
//! boundary's line style, the view and the area; Type Properties the fill
//! of a filled region type.

use crate::detail_ui::style_picker;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::tools::Tool;
use cc_core::region::Region;
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, TypeData};
use cc_engine::Ref;
use cc_engine::commands::region::{FilledRegionCreate, MaskingRegionCreate};
use cc_engine::commands::sketch::CurveArg;
use cc_views::format;
use egui::Ui;

/// Whether a sketch of `category` takes loops inside its outline as openings.
pub fn takes_openings(category: Category) -> bool {
    matches!(category, Category::Floor | Category::Roof | Category::FilledRegion | Category::MaskingRegion)
}

/// Start sketching a region of `category` in the active view. A project
/// without a type of it gets one when the first region is made.
pub fn start(state: &mut AppState, category: Category) {
    let type_id = state.project().default_type(category).unwrap_or(ElementTypeId(0));
    state.set_tool(Tool::Sketch { category, type_id });
    state.message = t!("hint-region");
}

/// Make the region the curves draw in the active view; returns its id.
pub fn create(
    state: &mut AppState,
    category: Category,
    type_id: ElementTypeId,
    curves: Vec<CurveArg>,
) -> Option<ElementId> {
    let Some(view) = state.active_view else {
        state.message = t!("why-cad-view");
        return None;
    };
    let view = Ref::Id(view.0);
    // The tool's type, if the project has it (none yet: the command makes one).
    let has_type = state.project().element_type(type_id).is_some_and(|t| t.category() == category);
    let out = if category == Category::MaskingRegion {
        crate::actions::run_command(state, MaskingRegionCreate { view, curves, boundary: None })
    } else {
        let type_ref = has_type.then_some(Ref::Id(type_id.0));
        crate::actions::run_command(state, FilledRegionCreate { view, curves, type_ref, boundary: None })
    };
    out.and_then(|o| o["id"].as_u64()).map(ElementId)
}

/// The properties of a selected region: its boundary style, view and size.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, region: &Region) {
    let styles = state.project().line_styles.clone();
    section(ui, "region", t!("section-graphics"), |ui| {
        let mut boundary = region.boundary;
        let changed = row(ui, &t!("prop-boundary-style"), |ui| {
            style_picker(ui, ("region-boundary", id.0), &styles, &mut boundary, true)
        });
        if changed {
            crate::engine_edit::element(state, id, None, |e| {
                if let ElementKind::FilledRegion(r) | ElementKind::MaskingRegion(r) = &mut e.kind {
                    r.boundary = boundary;
                }
            });
        }
        let view = state.project().view(region.view).map(|v| v.name.clone()).unwrap_or_default();
        readonly_row(ui, &t!("prop-view"), view);
    });
    section(ui, "dimensions", t!("section-dimensions"), |ui| {
        readonly_row(ui, &t!("prop-region-area"), format::area(region.area()));
        readonly_row(ui, &t!("prop-region-perimeter"), format::length(region.perimeter()));
        readonly_row(ui, &t!("prop-region-openings"), region.openings.len().to_string());
    });
}

/// The fill of a filled region type in Type Properties: the foreground
/// and background patterns and their colors.
pub fn type_section(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let TypeData::FilledRegion { fill } = ty.data else { return };
    let tid = ty.id;
    section(ui, "type-region-fill", t!("section-fill"), |ui| {
        let mut f = fill;
        for (layer, label) in [(0, t!("mat-foreground")), (1, t!("mat-background"))] {
            let (pattern, color) = if layer == 0 {
                (&mut f.foreground, &mut f.foreground_color)
            } else {
                (&mut f.background, &mut f.background_color)
            };
            row(ui, &format!("{label}: {}", t!("mat-pattern")), |ui| {
                crate::fill_patterns_dialog::picker(ui, ("region-pattern", layer), state.project(), pattern, 140.0);
            });
            row(ui, &format!("{label}: {}", t!("mat-color")), |ui| ui.color_edit_button_srgb(color));
        }
        row(ui, "", |ui| {
            if ui.button(t!("fp-manage")).on_hover_text(t!("cmd-desc-fill-patterns")).clicked() {
                crate::fill_patterns_dialog::open(state);
            }
        });
        if f != fill {
            // A color dragged in its picker stays one undo step.
            let merge = crate::graphics_dialog::key((tid, "region-fill"));
            crate::engine_edit::element_type(state, tid, Some(merge), |t| {
                if let TypeData::FilledRegion { fill } = &mut t.data {
                    *fill = f;
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_take_openings() {
        assert!(takes_openings(Category::FilledRegion) && takes_openings(Category::MaskingRegion));
        assert!(!takes_openings(Category::Ceiling));
    }
}
