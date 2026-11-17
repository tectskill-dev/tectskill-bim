//! Rows shared by the properties of member hosts (steel buildings,
//! trusses): the member counts and *Reset Members*, and length rows that
//! report when an edit ends.

use crate::t;
use crate::table::{readonly_row, row};
use cc_core::member_host::MemberHost;
use egui::Ui;
use std::ops::RangeInclusive;

/// Whether a drag or a typed value ended.
pub(crate) fn finished(r: &egui::Response) -> bool {
    r.drag_stopped() || r.lost_focus()
}

/// A length row; true when the edit ended.
pub(crate) fn length(ui: &mut Ui, label: &str, v: &mut f64, range: RangeInclusive<f64>) -> bool {
    finished(&row(ui, label, |ui| ui.add(crate::properties::length_drag(v).range(range))))
}

/// The members and detached counts and *Reset Members* (enabled when a
/// role is detached); true when it was clicked: the caller clears the
/// host's detached roles in its own edit.
pub(crate) fn member_rows(ui: &mut Ui, host: &dyn MemberHost) -> bool {
    readonly_row(ui, &t!("prop-members"), host.member_list().len().to_string());
    let detached = host.detached_roles().len();
    readonly_row(ui, &t!("prop-detached-members"), detached.to_string());
    ui.add_enabled(detached > 0, egui::Button::new(t!("action-reset-members"))).clicked()
}
