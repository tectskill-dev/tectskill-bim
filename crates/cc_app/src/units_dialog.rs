//! Project Units dialog: unit, decimals and symbol of each quantity, the
//! currency and the number style, with a live sample of each format.

use crate::dialog::{Dialog, session};
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::units::MAX_DECIMALS;
use cc_core::{AreaUnit, LengthUnit, NumberStyle, SlopeUnit, Units, VolumeUnit};
use cc_views::format;
use egui::{Context, DragValue, RichText, Ui};

/// Sample magnitude shown in the "Example" column, in display units.
const SAMPLE: f64 = 1234.5678;

fn set(state: &mut AppState, field: &str, f: impl FnOnce(&mut Units)) {
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("units", field).hash(&mut h);
        h.finish()
    };
    crate::engine_edit::units(state, Some(key), f);
}

fn done(state: &mut AppState, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

fn decimals(ui: &mut Ui, state: &mut AppState, field: &str, value: u8, apply: fn(&mut Units, u8)) {
    let mut v = value;
    let r = ui.add(DragValue::new(&mut v).range(0..=MAX_DECIMALS).speed(0.05));
    if r.changed() {
        set(state, field, |u| apply(u, v));
    }
    done(state, &r);
}

/// Precision of fractional inches: 1", 1/2", … 1/64" (stored as decimals d, 1/2^d).
fn precision(ui: &mut Ui, state: &mut AppState, value: u8) {
    let label = |d: u8| if d == 0 { "1\"".to_string() } else { format!("1/{}\"", 1u32 << d) };
    let mut v = value.min(MAX_DECIMALS);
    egui::ComboBox::from_id_salt("units-precision")
        .selected_text(label(v))
        .width(70.0)
        .show_ui(ui, |ui| {
            for d in 0..=MAX_DECIMALS {
                ui.selectable_value(&mut v, d, label(d));
            }
        })
        .response
        .on_hover_text(t!("units-precision-tip"));
    if v != value {
        set(state, "length-decimals", |u| u.length.decimals = v);
    }
}

fn symbol(ui: &mut Ui, state: &mut AppState, field: &str, value: bool, apply: fn(&mut Units, bool)) {
    let mut v = value;
    if crate::table::checkbox(ui, &mut v).changed() {
        set(state, field, |u| apply(u, v));
    }
}

fn unit_combo<U: Copy + PartialEq>(
    ui: &mut Ui,
    state: &mut AppState,
    id: &str,
    current: U,
    options: &[U],
    label: impl Fn(U) -> String,
    apply: fn(&mut Units, U),
) {
    let mut v = current;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(current))
        .width(if id == "style" { 240.0 } else { 150.0 })
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(&mut v, *o, label(*o));
            }
        });
    if v != current {
        set(state, id, |u| apply(u, v));
    }
}

fn length_name(u: LengthUnit) -> String {
    match u {
        LengthUnit::Millimeter => t!("unit-millimeter"),
        LengthUnit::Centimeter => t!("unit-centimeter"),
        LengthUnit::Meter => t!("unit-meter"),
        LengthUnit::Foot => t!("unit-foot"),
        LengthUnit::Inch => t!("unit-inch"),
        LengthUnit::FeetInches => t!("unit-feet-inches"),
        LengthUnit::FractionalInch => t!("unit-fractional-inch"),
    }
}

fn area_name(u: AreaUnit) -> String {
    match u {
        AreaUnit::SquareMeter => t!("unit-square-meter"),
        AreaUnit::SquareCentimeter => t!("unit-square-centimeter"),
        AreaUnit::SquareFoot => t!("unit-square-foot"),
        AreaUnit::SquareInch => t!("unit-square-inch"),
    }
}

fn volume_name(u: VolumeUnit) -> String {
    match u {
        VolumeUnit::CubicMeter => t!("unit-cubic-meter"),
        VolumeUnit::Liter => t!("unit-liter"),
        VolumeUnit::CubicFoot => t!("unit-cubic-foot"),
        VolumeUnit::CubicYard => t!("unit-cubic-yard"),
        VolumeUnit::Gallon => t!("unit-gallon"),
    }
}

fn slope_name(u: SlopeUnit) -> String {
    match u {
        SlopeUnit::Percent => t!("unit-percent"),
        SlopeUnit::Degrees => t!("unit-degrees"),
        SlopeUnit::RisePer12 => t!("unit-rise-per-12"),
    }
}

/// The number style's label with a sample number.
fn style_name(style: NumberStyle, comma: bool) -> String {
    let units = Units { style, ..Units::default() };
    let sample = format::number_with(&units, comma, 1234567.891, 2);
    match style {
        NumberStyle::Language => format!("{} ({sample})", t!("units-style-language")),
        _ => sample,
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    if !state.dialogs.units {
        return;
    }
    session::begin(state);
    let (mut ok, mut reset, mut imperial) = (0, 0, 0);
    let response = Dialog::new("units", t!("dlg-units")).width(620.0).show(
        ctx,
        |ui| body(ui, state),
        |f| {
            reset = f.extra(t!("units-reset"));
            imperial = f.extra(t!("units-imperial"));
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked == Some(reset) {
        crate::engine_edit::units(state, None, |u| *u = Units::default());
        return;
    }
    if response.clicked == Some(imperial) {
        crate::engine_edit::units(state, None, |u| *u = Units::imperial());
        return;
    }
    if crate::dialogs::ok_cancel(state, &response, ok) {
        state.dialogs.units = false;
    }
}

fn body(ui: &mut Ui, state: &mut AppState) {
    let u = state.project().units.clone();
    let comma = crate::i18n::language().decimal_comma();
    ui.label(theme::muted(t!("units-intro")));
    ui.add_space(6.0);
    egui::Grid::new("units-grid").num_columns(5).striped(true).spacing([14.0, 8.0]).show(ui, |ui| {
        for h in ["units-quantity", "units-unit", "units-decimals", "units-symbol", "units-example"] {
            ui.label(RichText::new(tr(h)).color(PALETTE.text_muted).size(12.0));
        }
        ui.end_row();

        ui.label(crate::properties::field_label(cc_core::ScheduleField::Length));
        unit_combo(ui, state, "length", u.length.unit, &LengthUnit::ALL, length_name, |u, v| u.length.unit = v);
        if u.length.unit.is_fractional() {
            precision(ui, state, u.length.decimals);
            // Feet and inches always show their marks.
            ui.label("");
        } else {
            decimals(ui, state, "length-decimals", u.length.decimals, |u, v| u.length.decimals = v);
            symbol(ui, state, "length-symbol", u.length.symbol, |u, v| u.length.symbol = v);
        }
        let sample = SAMPLE / u.length.unit.per_meter();
        ui.label(RichText::new(format::length_with(&u, comma, sample)).monospace());
        ui.end_row();

        ui.label(crate::properties::field_label(cc_core::ScheduleField::Area));
        unit_combo(ui, state, "area", u.area.unit, &AreaUnit::ALL, area_name, |u, v| u.area.unit = v);
        decimals(ui, state, "area-decimals", u.area.decimals, |u, v| u.area.decimals = v);
        symbol(ui, state, "area-symbol", u.area.symbol, |u, v| u.area.symbol = v);
        let sample = SAMPLE / u.area.unit.per_square_meter();
        ui.label(RichText::new(format::area_with(&u, comma, sample)).monospace());
        ui.end_row();

        ui.label(crate::properties::field_label(cc_core::ScheduleField::Volume));
        unit_combo(ui, state, "volume", u.volume.unit, &VolumeUnit::ALL, volume_name, |u, v| u.volume.unit = v);
        decimals(ui, state, "volume-decimals", u.volume.decimals, |u, v| u.volume.decimals = v);
        symbol(ui, state, "volume-symbol", u.volume.symbol, |u, v| u.volume.symbol = v);
        let sample = SAMPLE / u.volume.unit.per_cubic_meter();
        ui.label(RichText::new(format::volume_with(&u, comma, sample)).monospace());
        ui.end_row();

        ui.label(t!("units-angle"));
        ui.label(t!("unit-degrees"));
        decimals(ui, state, "angle-decimals", u.angle.decimals, |u, v| u.angle.decimals = v);
        symbol(ui, state, "angle-symbol", u.angle.symbol, |u, v| u.angle.symbol = v);
        ui.label(RichText::new(format::angle_with(&u, comma, 12.3456)).monospace());
        ui.end_row();

        ui.label(t!("units-slope"));
        unit_combo(ui, state, "slope", u.slope.unit, &SlopeUnit::ALL, slope_name, |u, v| u.slope.unit = v);
        decimals(ui, state, "slope-decimals", u.slope.decimals, |u, v| u.slope.decimals = v);
        symbol(ui, state, "slope-symbol", u.slope.symbol, |u, v| u.slope.symbol = v);
        ui.label(RichText::new(format::slope_with(&u, comma, 0.2345)).monospace());
        ui.end_row();

        ui.label(t!("units-currency"));
        let mut currency = u.currency.clone();
        let r = crate::table::live_text(ui, "units-currency", &mut currency, |ui, c| {
            ui.add(egui::TextEdit::singleline(c).desired_width(150.0).hint_text(t!("units-currency-hint")))
        });
        if r.changed() {
            let currency: String = currency.chars().take(8).collect();
            set(state, "currency", |u| u.currency = currency);
        }
        done(state, &r);
        decimals(ui, state, "currency-decimals", u.currency_decimals, |u, v| u.currency_decimals = v);
        ui.label("");
        ui.label(RichText::new(format::money_with(&u, comma, SAMPLE)).monospace());
        ui.end_row();
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(t!("units-style"));
        unit_combo(ui, state, "style", u.style, &NumberStyle::ALL, |s| style_name(s, comma), |u, v| u.style = v);
    });
}
