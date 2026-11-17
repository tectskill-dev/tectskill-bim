//! Trusses in the app: the Truss tool's options bar and the properties of a
//! selected truss (every parameter through `element_set`, which regenerates
//! its members in the same undo step).

use crate::grid_frame_ui::combo;
use crate::member_host_ui::{finished, length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use cc_core::truss::{self as tr, Truss, TrussPattern, TrussShape};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId};
use egui::{DragValue, Ui};

/// The patterns offered for a shape (gable-only ones on gables).
pub fn patterns_for(shape: TrussShape) -> Vec<TrussPattern> {
    TrussPattern::ALL.into_iter().filter(|p| !p.needs_gable() || shape == TrussShape::Gable).collect()
}

fn shapes() -> Vec<(TrussShape, String)> {
    TrussShape::ALL.into_iter().map(|s| (s, crate::i18n::tr(s.key()))).collect()
}

fn patterns(shape: TrussShape) -> Vec<(TrussPattern, String)> {
    patterns_for(shape).into_iter().map(|p| (p, crate::i18n::tr(p.key()))).collect()
}

/// `panels` after an edit: even on a gable, stepping the way it moved from `before`.
fn even_on_gable(shape: TrussShape, panels: u32, before: u32) -> u32 {
    if shape != TrussShape::Gable || panels.is_multiple_of(2) {
        return panels;
    }
    let stepped = if panels < before { panels.saturating_sub(1) } else { panels + 1 };
    stepped.clamp(*tr::PANELS.start(), *tr::PANELS.end())
}

/// The options bar of the Truss tool.
pub fn tool_options(ui: &mut Ui, state: &mut AppState) {
    let o = &mut state.truss;
    ui.label(t!("prop-shape"));
    combo(ui, "truss-shape", &mut o.shape, &shapes());
    let offered = patterns(o.shape);
    if !offered.iter().any(|(p, _)| *p == o.pattern) {
        o.pattern = TrussPattern::Pratt;
    }
    ui.label(t!("prop-truss-pattern"));
    combo(ui, "truss-pattern", &mut o.pattern, &offered);
    ui.label(t!("prop-truss-height"));
    ui.add(crate::properties::length_drag(&mut o.height).speed(0.01).range(tr::HEIGHT));
    ui.label(t!("prop-truss-panels"));
    ui.add(
        DragValue::new(&mut o.panels)
            .range(0..=*tr::PANELS.end())
            .custom_formatter(|n, _| if n < 0.5 { t!("truss-panels-auto") } else { format!("{n:.0}") }),
    );
    ui.label(t!("prop-truss-copies"));
    ui.add(DragValue::new(&mut o.copies).range(tr::COPIES));
    ui.label(t!("label-spacing"));
    ui.add(crate::properties::length_drag(&mut o.spacing).speed(0.01).range(tr::SPACING));
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-truss")));
}

/// Change truss `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut Truss)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::Truss(t) = &mut e.kind {
            f(t);
        }
    });
}

/// The rows of a selected truss.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, truss: &Truss) {
    let mut t = truss.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|x| (x.id, x.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    section(ui, "truss", t!("prop-truss"), |ui| {
        let shape = t.shape;
        row(ui, &t!("prop-shape"), |ui| combo(ui, &format!("truss-shape-{}", id.0), &mut t.shape, &shapes()));
        if t.shape != shape {
            t.panels = even_on_gable(t.shape, t.panels, t.panels);
            if !patterns_for(t.shape).contains(&t.pattern) {
                t.pattern = TrussPattern::Pratt;
            }
            done = true;
        }
        let pattern = t.pattern;
        let offered = patterns(t.shape);
        row(ui, &t!("prop-truss-pattern"), |ui| {
            combo(ui, &format!("truss-pattern-{}", id.0), &mut t.pattern, &offered)
        });
        done |= t.pattern != pattern;
        done |= length(ui, &t!("prop-truss-height"), &mut t.height, tr::HEIGHT);
        let panels = t.panels;
        done |=
            finished(&row(ui, &t!("prop-truss-panels"), |ui| ui.add(DragValue::new(&mut t.panels).range(tr::PANELS))));
        t.panels = even_on_gable(t.shape, t.panels, panels);
        done |= length(ui, &t!("prop-base-offset"), &mut t.base_offset, tr::BASE_OFFSET);
        readonly_row(ui, &t!("prop-length"), cc_views::format::length(truss.length()));
        done |=
            finished(&row(ui, &t!("prop-truss-copies"), |ui| ui.add(DragValue::new(&mut t.copies).range(tr::COPIES))));
        done |= length(ui, &t!("label-spacing"), &mut t.spacing, tr::SPACING);
        if member_rows(ui, truss) {
            t.detached.clear();
            done = true;
        }
    });
    section(ui, "truss-types", t!("section-type"), |ui| {
        let before = (t.top_chord_type, t.bottom_chord_type, t.web_type, t.post_type);
        row(ui, &t!("label-top-chord-type"), |ui| {
            combo(ui, &format!("truss-top-{}", id.0), &mut t.top_chord_type, &beams)
        });
        row(ui, &t!("label-bottom-chord-type"), |ui| {
            combo(ui, &format!("truss-bottom-{}", id.0), &mut t.bottom_chord_type, &beams)
        });
        row(ui, &t!("label-web-type"), |ui| combo(ui, &format!("truss-web-{}", id.0), &mut t.web_type, &beams));
        row(ui, &t!("label-post-type"), |ui| combo(ui, &format!("truss-post-{}", id.0), &mut t.post_type, &columns));
        done |= (t.top_chord_type, t.bottom_chord_type, t.web_type, t.post_type) != before;
    });
    if t != *truss {
        edit(state, id, "truss", move |x| *x = t);
    }
    if done {
        state.session.doc.break_merge();
    }
}
