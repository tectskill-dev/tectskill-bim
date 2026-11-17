//! The Type Properties rows of a dimension type: the marks, witness and
//! dimension lines and pens (Graphics) and the text (Text).

use crate::properties::{paper_drag, weight_drag};
use crate::state::AppState;
use crate::t;
use crate::table::{row, section};
use cc_core::detail::{MAX_WEIGHT_MM, MIN_WEIGHT_MM};
use cc_core::dim_style::{DimensionStyle, MAX_AFFIX, MAX_LENGTH_MM, MIN_MARK_MM};
use cc_core::{ElementType, TypeData};
use egui::{Response, Ui};

/// What one frame's rows did.
#[derive(Default)]
struct Edits {
    changed: bool,
    done: bool,
}

impl Edits {
    fn add(&mut self, r: &Response) {
        self.changed |= r.changed();
        self.done |= r.drag_stopped() || r.lost_focus();
    }
}

/// A paper-millimeter length row.
fn mm_row(ui: &mut Ui, edits: &mut Edits, label: &str, value: &mut f64, min: f64) {
    let r = row(ui, label, |ui| ui.add(paper_drag(value).speed(0.05).range(min..=MAX_LENGTH_MM)));
    edits.add(&r);
}

/// An optional value: a checkbox, and the value's editor when checked.
fn optional_row<T: Copy>(
    ui: &mut Ui,
    edits: &mut Edits,
    label: &str,
    value: &mut Option<T>,
    default: T,
    editor: impl FnOnce(&mut Ui, &mut T) -> Response,
) {
    let r = row(ui, label, |ui| {
        ui.horizontal(|ui| {
            let mut on = value.is_some();
            let mut r = crate::table::checkbox(ui, &mut on);
            if on != value.is_some() {
                *value = on.then_some(default);
                // A click is one whole edit.
                r.mark_changed();
                edits.done = true;
            }
            match value {
                Some(v) => r | editor(ui, v),
                None => r,
            }
        })
        .inner
    });
    edits.add(&r);
}

fn weight_row(ui: &mut Ui, edits: &mut Edits, label: &str, value: &mut Option<f64>) {
    optional_row(ui, edits, label, value, 0.25, |ui, w| {
        ui.add(weight_drag(w).speed(0.01).range(MIN_WEIGHT_MM..=MAX_WEIGHT_MM))
    });
}

fn text_row(ui: &mut Ui, edits: &mut Edits, label: &str, text: &mut String) {
    let r = row(ui, label, |ui| ui.add(egui::TextEdit::singleline(text).char_limit(MAX_AFFIX)));
    edits.add(&r);
}

/// The dimension type's sections; nothing for other types.
pub fn type_sections(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let TypeData::Dimension { text_size, style } = &ty.data else { return };
    let (mut size, mut s): (f64, DimensionStyle) = (*text_size, style.clone());
    let mut edits = Edits::default();
    section(ui, "type-dimension-graphics", t!("section-graphics"), |ui| {
        let r = row(ui, &t!("prop-dim-mark"), |ui| {
            let before = s.mark;
            let mut r = ui.push_id("dim-mark", |ui| {
                crate::text_tool::symbol_combo(ui, "dim-mark", &mut s.mark);
            });
            if s.mark != before {
                r.response.mark_changed();
                edits.done = true;
            }
            r.response
        });
        edits.add(&r);
        mm_row(ui, &mut edits, &t!("prop-dim-mark-size"), &mut s.mark_size, MIN_MARK_MM);
        weight_row(ui, &mut edits, &t!("prop-dim-mark-weight"), &mut s.mark_weight);
        weight_row(ui, &mut edits, &t!("prop-dim-line-weight"), &mut s.line_weight);
        optional_row(ui, &mut edits, &t!("mat-color"), &mut s.color, [0, 0, 0], |ui, c| ui.color_edit_button_srgb(c));
        mm_row(ui, &mut edits, &t!("prop-dim-witness-gap"), &mut s.witness_gap, 0.0);
        mm_row(ui, &mut edits, &t!("prop-dim-witness-extension"), &mut s.witness_extension, 0.0);
        mm_row(ui, &mut edits, &t!("prop-dim-line-extension"), &mut s.line_extension, 0.0);
    });
    section(ui, "type-dimension-text", t!("section-text"), |ui| {
        let r =
            row(ui, &t!("prop-text-size"), |ui| ui.add(paper_drag(&mut size).speed(0.1).range(0.5..=MAX_LENGTH_MM)));
        edits.add(&r);
        mm_row(ui, &mut edits, &t!("prop-dim-text-offset"), &mut s.text_offset, 0.0);
        text_row(ui, &mut edits, &t!("prop-dim-prefix"), &mut s.prefix);
        text_row(ui, &mut edits, &t!("prop-dim-suffix"), &mut s.suffix);
        let r = row(ui, &t!("prop-dim-opaque-text"), |ui| crate::table::checkbox(ui, &mut s.opaque_text));
        edits.done |= r.changed();
        edits.add(&r);
    });
    if edits.changed {
        let key = crate::properties::key((ty.id, "dimension-type"));
        crate::engine_edit::element_type(state, ty.id, Some(key), |t| {
            if let TypeData::Dimension { text_size, style } = &mut t.data {
                (*text_size, *style) = (size, s);
            }
        });
    }
    if edits.done {
        state.session.doc.break_merge();
    }
}
