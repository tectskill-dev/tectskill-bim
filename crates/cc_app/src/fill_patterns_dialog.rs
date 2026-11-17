//! The Fill Patterns dialog (*Manage → Fill Patterns*, also opened from the
//! material browser and a filled region type's fill): the built-in
//! patterns, read only, and the project's own with a drawn swatch each;
//! the selected one's name, description, kind (drafting or model), scale
//! and line families, edited in place over a live preview. *Import .pat…*
//! reads a file and lets the user pick which of its patterns come in;
//! *New*, *Duplicate* and *Delete* (with the pattern that takes its place
//! where it is used) complete it. Every change runs a `fill_pattern_*`
//! command; OK keeps them, Cancel undoes them all (`dialog::session`,
//! through [`crate::graphics_dialog::Edits`]).
//!
//! The module also holds what the pattern pickers share: labels, swatches
//! and the picker itself, listing custom patterns after the built-ins.

use crate::dialog::Dialog;
use crate::graphics_dialog::{Edits, header, key, run};
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::fill_pattern::{FillPattern, FillPatternKind, MAX_SCALE, MIN_SCALE, PatternLine};
use cc_core::{FillPatternId, Pattern, Project};
use cc_engine::Ref;
use cc_engine::commands::fill_patterns as cmd;
use egui::{Color32, Context, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};
use glam::DVec2;
use std::path::PathBuf;

/// Screen points per paper millimeter in previews.
const PREVIEW_MM: f64 = 2.5;
/// The view scale model patterns are previewed at (1:50).
const PREVIEW_SCALE: f64 = 50.0;

// ---------------------------------------------------------------- shared by the pickers

/// A pattern's name: the built-in's, translated, or the project pattern's.
pub fn label(project: &Project, p: Pattern) -> String {
    match p {
        Pattern::Custom(id) => project.fill_pattern(id).map_or_else(|| tr(p.key()), |f| f.name.clone()),
        b => tr(b.key()),
    }
}

/// The patterns a picker offers: the built-in ones, then the project's.
pub fn choices(project: &Project) -> Vec<(Pattern, String)> {
    let built_in = Pattern::ALL.iter().map(|p| (*p, tr(p.key())));
    let own = project.fill_patterns.values().map(|f| (Pattern::Custom(f.id), f.name.clone()));
    built_in.chain(own).collect()
}

/// Lines of `fp` inside `rect` for a preview: drafting patterns at their
/// paper size, model patterns as at 1:50 (y up, as drawn).
fn custom_lines(fp: &FillPattern, rect: Rect) -> Vec<(Pos2, Pos2)> {
    let unit = match fp.kind {
        FillPatternKind::Drafting => PREVIEW_MM,
        FillPatternKind::Model => PREVIEW_MM / PREVIEW_SCALE,
    } * fp.scale;
    // Flip y so angles turn counterclockwise on screen, as on paper.
    let poly: Vec<DVec2> = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
        .iter()
        .map(|p| DVec2::new(p.x as f64, -(p.y as f64)))
        .collect();
    let to = |p: DVec2| Pos2::new(p.x as f32, -p.y as f32);
    cc_views::hatch::hatch_custom(fp, &poly, unit, PREVIEW_MM).into_iter().map(|(a, b)| (to(a), to(b))).collect()
}

/// Paint `pattern` in `color` over `rect` (built-ins with lines `spacing`
/// points apart); `Solid` fills it.
pub fn paint(painter: &egui::Painter, rect: Rect, project: &Project, pattern: Pattern, color: [u8; 3], spacing: f64) {
    let c = Color32::from_rgb(color[0], color[1], color[2]);
    let stroke = Stroke::new(1.0, c);
    match pattern {
        Pattern::None => {}
        Pattern::Solid => {
            painter.rect_filled(rect, 0.0, c);
        }
        Pattern::Custom(id) => {
            let Some(fp) = project.fill_pattern(id) else { return };
            for (a, b) in custom_lines(fp, rect) {
                painter.line_segment([a, b], stroke);
            }
        }
        b => {
            let poly: Vec<DVec2> = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
                .iter()
                .map(|p| DVec2::new(p.x as f64, p.y as f64))
                .collect();
            for (a, b) in cc_views::hatch::hatch(b, &poly, spacing) {
                painter.line_segment([Pos2::new(a.x as f32, a.y as f32), Pos2::new(b.x as f32, b.y as f32)], stroke);
            }
        }
    }
}

/// A small framed swatch of `pattern` in black on white.
pub fn swatch(ui: &mut Ui, project: &Project, pattern: Pattern, size: egui::Vec2) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 1.0, Color32::WHITE);
    paint(&painter, rect.shrink(1.0), project, pattern, [40, 40, 40], 5.0);
    ui.painter().rect_stroke(rect, 1.0, Stroke::new(1.0, PALETTE.border_strong), egui::StrokeKind::Inside);
    r
}

/// A drop-down of every pattern (custom ones after the built-ins, each
/// with its swatch); returns whether the choice changed.
pub fn picker(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    project: &Project,
    value: &mut Pattern,
    width: f32,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        swatch(ui, project, *value, vec2(28.0, 16.0));
        egui::ComboBox::from_id_salt(id).selected_text(label(project, *value)).width(width).show_ui(ui, |ui| {
            let mut divided = false;
            for (p, name) in choices(project) {
                // A divider before the project's patterns.
                if matches!(p, Pattern::Custom(_)) && !divided {
                    ui.separator();
                    divided = true;
                }
                ui.horizontal(|ui| {
                    swatch(ui, project, p, vec2(24.0, 14.0));
                    ui.selectable_value(value, p, name);
                });
            }
        });
    });
    *value != before
}

// ---------------------------------------------------------------- the dialog

/// The open Fill Patterns dialog.
pub struct FillPatternsDialog {
    selected: Pattern,
    /// A text being typed: which field of which pattern, and the text.
    draft: Option<(u64, String)>,
    /// The pattern that takes a deleted one's place where it is used.
    replace_with: Pattern,
    import: Option<ImportDialog>,
    edits: Edits,
}

/// The patterns of a `.pat` file, before they come in.
struct ImportDialog {
    path: PathBuf,
    entries: Vec<Entry>,
    errors: Vec<String>,
    /// Every pattern's kind, or the file's markers (`None`).
    kind: Option<FillPatternKind>,
    /// `auto`, `mm` or `inch`.
    units: &'static str,
    scale: f64,
    on_clash: cmd::OnClash,
}

struct Entry {
    pattern: FillPattern,
    exists: bool,
    pick: bool,
}

pub fn open(state: &mut AppState) {
    let edits = Edits::begin(state);
    let selected = state.project().fill_patterns.keys().next().map_or(Pattern::Diagonal, |id| Pattern::Custom(*id));
    state.dialogs.fill_patterns =
        Some(FillPatternsDialog { selected, draft: None, replace_with: Pattern::None, import: None, edits });
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(d) = state.dialogs.fill_patterns.as_mut() else { return };
    let (mut selected, mut draft, mut replace_with) = (d.selected, d.draft.take(), d.replace_with);
    if !state.project().has_pattern(selected) {
        selected = Pattern::Diagonal;
    }
    let mut pick_file = false;
    let mut ok = 0;
    let response = Dialog::new("fill-patterns", t!("dlg-fill-patterns")).width(820.0).show(
        ctx,
        |ui| {
            ui.label(RichText::new(t!("fp-hint")).size(11.0).color(PALETTE.text_faint));
            ui.add_space(4.0);
            crate::dialog::two_columns(ui, [250.0, 560.0], |ui, column| {
                if column == 0 {
                    list(ui, state, &mut selected);
                    ui.add_space(6.0);
                    pick_file = buttons(ui, state, &mut selected, &mut replace_with);
                } else {
                    details(ui, state, selected, &mut draft);
                }
            });
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if let Some(d) = state.dialogs.fill_patterns.as_mut() {
        (d.selected, d.draft, d.replace_with) = (selected, draft, replace_with);
    }
    if pick_file {
        pick_pat_file(state);
    }
    import_dialog(ctx, state);
    let close = match response.clicked {
        Some(i) => Some(i == ok),
        None if response.dismissed && state.dialogs.fill_patterns.as_ref().is_some_and(|d| d.import.is_none()) => {
            Some(false)
        }
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.fill_patterns.take()
    {
        d.edits.finish(state, keep);
    }
}

fn list(ui: &mut Ui, state: &AppState, selected: &mut Pattern) {
    let project = state.project();
    egui::Frame::new().stroke(Stroke::new(1.0, PALETTE.border)).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("fp-list").max_height(420.0).auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(t!("fp-built-in")).color(PALETTE.text_muted).italics());
            let mut headed = false;
            for (p, name) in choices(project) {
                if matches!(p, Pattern::Custom(_)) && !headed {
                    headed = true;
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("fp-project")).color(PALETTE.text_muted).italics());
                }
                ui.horizontal(|ui| {
                    swatch(ui, project, p, vec2(32.0, 18.0));
                    let text = if matches!(p, Pattern::Custom(_)) {
                        RichText::new(name)
                    } else {
                        RichText::new(name).color(PALETTE.text_muted)
                    };
                    if ui.selectable_label(*selected == p, text).clicked() {
                        *selected = p;
                    }
                });
            }
            if project.fill_patterns.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(t!("fp-none")).color(PALETTE.text_faint));
            }
        });
    });
}

/// The list's buttons; returns whether *Import .pat…* was pressed.
fn buttons(ui: &mut Ui, state: &mut AppState, selected: &mut Pattern, replace_with: &mut Pattern) -> bool {
    let mut import = false;
    ui.horizontal_wrapped(|ui| {
        import = ui.button(t!("fp-import")).on_hover_text(t!("fp-import-tip")).clicked();
        if ui.button(t!("fp-new")).on_hover_text(t!("fp-new-tip")).clicked() {
            let names: Vec<String> = state.project().fill_patterns.values().map(|f| f.name.to_lowercase()).collect();
            let name = (1..1000)
                .map(|n| t!("fp-default-name", "n" => n))
                .find(|n| !names.contains(&n.to_lowercase()))
                .unwrap_or_default();
            let line = cmd::LineArg { angle: 45.0, offset: [0.0, 3.0], ..Default::default() };
            let command = cmd::FillPatternCreate { name, lines: vec![line], ..Default::default() };
            if let Some(out) = crate::actions::run_command(state, command)
                && let Some(id) = out["id"].as_u64()
            {
                *selected = Pattern::Custom(FillPatternId(id));
            }
        }
        let custom = match *selected {
            Pattern::Custom(id) => Some(id),
            _ => None,
        };
        if ui.add_enabled(custom.is_some(), egui::Button::new(t!("fp-duplicate"))).clicked()
            && let Some(id) = custom
            && let Some(out) =
                crate::actions::run_command(state, cmd::FillPatternDuplicate { pattern: Ref::Id(id.0), name: None })
            && let Some(new) = out["id"].as_u64()
        {
            *selected = Pattern::Custom(FillPatternId(new));
        }
        let used = custom.map_or(0, |id| state.project().fill_pattern_usage(id).count());
        let delete = ui
            .add_enabled(custom.is_some(), egui::Button::new(t!("fp-delete")))
            .on_hover_text(t!("fp-delete-tip"))
            .clicked();
        if delete && let Some(id) = custom {
            let replace = (used > 0).then(|| cmd::pattern_code(state.project(), *replace_with));
            if run(state, None, cmd::FillPatternDelete { pattern: Ref::Id(id.0), replace_with: replace }) {
                *selected = Pattern::Diagonal;
            }
        }
        if used > 0 {
            ui.end_row();
            ui.label(theme::muted(t!("fp-replace-with", "count" => used)));
            let mut choice = *replace_with;
            let project = state.project();
            egui::ComboBox::from_id_salt("fp-replace").selected_text(label(project, choice)).width(120.0).show_ui(
                ui,
                |ui| {
                    for (p, name) in
                        choices(project).into_iter().filter(|(p, _)| Some(*p) != custom.map(Pattern::Custom))
                    {
                        ui.selectable_value(&mut choice, p, name);
                    }
                },
            );
            *replace_with = choice;
        }
    });
    import
}

fn row_label(ui: &mut Ui, text: String) {
    ui.label(theme::muted(text));
}

fn details(ui: &mut Ui, state: &mut AppState, selected: Pattern, draft: &mut Option<(u64, String)>) {
    let Pattern::Custom(id) = selected else {
        ui.label(RichText::new(label(state.project(), selected)).strong());
        ui.label(theme::muted(t!("fp-read-only")));
        ui.add_space(6.0);
        preview(ui, state.project(), selected);
        return;
    };
    let Some(fp) = state.project().fill_pattern(id).cloned() else { return };
    let pattern = Ref::Id(id.0);
    egui::Grid::new("fp-props").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        row_label(ui, t!("prop-name"));
        if let Some(text) = text_field(ui, draft, key((id, "name")), &fp.name, 260.0) {
            run(state, None, cmd::FillPatternSet { pattern: pattern.clone(), name: Some(text), ..Default::default() });
        }
        ui.end_row();
        row_label(ui, t!("fp-description"));
        if let Some(text) = text_field(ui, draft, key((id, "description")), &fp.description, 260.0) {
            let command =
                cmd::FillPatternSet { pattern: pattern.clone(), description: Some(text), ..Default::default() };
            run(state, None, command);
        }
        ui.end_row();
        row_label(ui, t!("fp-kind"));
        let mut kind = fp.kind;
        ui.horizontal(|ui| {
            for k in [FillPatternKind::Drafting, FillPatternKind::Model] {
                ui.radio_value(&mut kind, k, tr(k.key()));
            }
        });
        if kind != fp.kind {
            let command =
                cmd::FillPatternSet { pattern: pattern.clone(), kind: Some(kind.code().into()), ..Default::default() };
            run(state, None, command);
        }
        ui.end_row();
        row_label(ui, t!("fp-scale"));
        let mut scale = fp.scale;
        let r = ui.add(egui::DragValue::new(&mut scale).speed(0.01).range(MIN_SCALE..=MAX_SCALE));
        if r.changed() {
            let command = cmd::FillPatternSet { pattern: pattern.clone(), scale: Some(scale), ..Default::default() };
            run(state, Some(key((id, "scale"))), command);
        }
        if r.drag_stopped() || r.lost_focus() {
            state.session.doc.break_merge();
        }
        ui.end_row();
        row_label(ui, t!("fp-used-by"));
        ui.label(state.project().fill_pattern_usage(id).count().to_string());
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t!("fp-families")).strong());
    let hint = if in_inches(&fp) { t!("fp-families-hint-inches") } else { t!("fp-families-hint") };
    ui.label(RichText::new(hint).size(11.0).color(PALETTE.text_faint));
    families(ui, state, &fp, draft);
    ui.add_space(6.0);
    preview(ui, state.project(), selected);
}

/// A single-line field that commits when it loses focus; returns the text then.
fn text_field(ui: &mut Ui, draft: &mut Option<(u64, String)>, field: u64, value: &str, width: f32) -> Option<String> {
    let mut text = match draft {
        Some((f, t)) if *f == field => t.clone(),
        _ => value.to_string(),
    };
    let r = ui.add_sized([width, 22.0], egui::TextEdit::singleline(&mut text));
    if r.changed() {
        *draft = Some((field, text.clone()));
    }
    if r.lost_focus() {
        if draft.as_ref().is_some_and(|(f, _)| *f == field) {
            *draft = None;
        }
        if text.trim() != value.trim() {
            return Some(text);
        }
    }
    None
}

fn line_arg(l: &PatternLine) -> cmd::LineArg {
    cmd::LineArg { angle: l.angle, origin: Some(l.origin), offset: l.offset, dashes: Some(l.dashes.clone()) }
}

/// Whether a pattern's lengths read in inches: a drafting pattern (paper
/// millimeters) of an imperial project. Model patterns stay millimeters.
fn in_inches(fp: &FillPattern) -> bool {
    fp.kind == FillPatternKind::Drafting && cc_views::format::imperial()
}

/// Millimeters in an inch, for paper lengths.
const PAPER_INCH: f64 = 25.4;

/// Dash lengths typed as numbers separated by commas or spaces, in
/// millimeters (inches when `inches`).
fn parse_dashes(text: &str, inches: bool) -> Option<Vec<f64>> {
    let per = if inches { PAPER_INCH } else { 1.0 };
    text.split([',', ' ', ';'])
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v * per))
        .collect()
}

fn dashes_text(d: &[f64], inches: bool) -> String {
    let text = |v: f64| {
        if !inches {
            return format!("{v}");
        }
        // Inches to four places, trailing zeros dropped.
        let s = format!("{:.4}", v / PAPER_INCH);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" { "0".to_string() } else { s.to_string() }
    };
    d.iter().map(|v| text(*v)).collect::<Vec<_>>().join(", ")
}

/// The table of line families: angle, origin, offset, dashes, and a row
/// remover; *Add line family* below.
fn families(ui: &mut Ui, state: &mut AppState, fp: &FillPattern, draft: &mut Option<(u64, String)>) {
    let id = fp.id;
    let inches = in_inches(fp);
    let mut lines = fp.lines.clone();
    let mut changed: Option<Option<u64>> = None;
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("fp-families").max_height(180.0).show(ui, |ui| {
        egui::Grid::new("fp-families-grid").num_columns(7).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
            for h in [
                t!("fp-angle"),
                t!("fp-origin-x"),
                t!("fp-origin-y"),
                t!("fp-shift"),
                t!("fp-spacing"),
                t!("fp-dashes"),
            ] {
                header(ui, h);
            }
            ui.label("");
            ui.end_row();
            for (i, l) in lines.iter_mut().enumerate() {
                let mut drag = |ui: &mut Ui, v: &mut f64, field: &str, suffix: &str| {
                    // Lengths of drafting patterns in imperial projects in decimal inches.
                    let r = if inches && field != "angle" {
                        ui.add(crate::properties::decimal_paper_drag(v).speed(0.1))
                    } else {
                        ui.add(egui::DragValue::new(v).speed(0.1).max_decimals(4).suffix(suffix))
                    };
                    if r.changed() {
                        changed = Some(Some(key((id, i, field.to_string()))));
                    }
                    if r.drag_stopped() || r.lost_focus() {
                        state.session.doc.break_merge();
                    }
                };
                drag(ui, &mut l.angle, "angle", "°");
                drag(ui, &mut l.origin[0], "x", "");
                drag(ui, &mut l.origin[1], "y", "");
                drag(ui, &mut l.offset[0], "shift", "");
                drag(ui, &mut l.offset[1], "spacing", "");
                let field = key((id, i, "dashes"));
                if let Some(text) = text_field(ui, draft, field, &dashes_text(&l.dashes, inches), 130.0) {
                    match parse_dashes(&text, inches) {
                        Some(d) => {
                            l.dashes = d;
                            changed = Some(None);
                        }
                        None => state.message = t!("fp-bad-dashes"),
                    }
                }
                let can_remove = fp.lines.len() > 1;
                if ui
                    .add_enabled(can_remove, egui::Button::new("×").small())
                    .on_hover_text(t!("fp-remove-family"))
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if ui.button(t!("fp-add-family")).clicked() {
        let last = lines.last().cloned().unwrap_or_else(|| PatternLine::solid(0.0, 3.0));
        lines.push(PatternLine { angle: last.angle + 90.0, ..last });
        changed = Some(None);
    }
    if let Some(i) = remove {
        lines.remove(i);
        changed = Some(None);
    }
    if let Some(merge) = changed {
        let command = cmd::FillPatternSet {
            pattern: Ref::Id(id.0),
            lines: Some(lines.iter().map(line_arg).collect()),
            ..Default::default()
        };
        run(state, merge, command);
    }
}

/// A larger preview of `pattern`, as drawn.
fn preview(ui: &mut Ui, project: &Project, pattern: Pattern) {
    ui.label(theme::muted(t!("mat-preview")));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(540.0), 120.0), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    paint(&painter, rect, project, pattern, [30, 30, 30], 7.0);
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, PALETTE.border_strong), egui::StrokeKind::Inside);
    if let Pattern::Custom(id) = pattern
        && let Some(fp) = project.fill_pattern(id)
    {
        let note = match fp.kind {
            FillPatternKind::Drafting => t!("fp-preview-drafting"),
            FillPatternKind::Model => t!("fp-preview-model"),
        };
        ui.label(RichText::new(note).size(11.0).color(PALETTE.text_faint));
    }
}

// ---------------------------------------------------------------- importing

fn pick_pat_file(state: &mut AppState) {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("file-filter-pat"), &["pat", "PAT"]);
    if let Some(dir) = state.prefs.projects_dir() {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else { return };
    open_import(state, path);
}

/// Read the patterns of the `.pat` file at `path` and offer them.
pub fn open_import(state: &mut AppState, path: PathBuf) {
    let mut import = ImportDialog {
        path,
        entries: Vec::new(),
        errors: Vec::new(),
        kind: None,
        units: "auto",
        scale: 1.0,
        on_clash: cmd::OnClash::Rename,
    };
    if read(state, &mut import)
        && let Some(d) = state.dialogs.fill_patterns.as_mut()
    {
        d.import = Some(import);
    }
}

/// The import's command with its options.
fn command(i: &ImportDialog, preview: bool, names: Option<Vec<String>>) -> cmd::FillPatternImport {
    cmd::FillPatternImport {
        path: Some(i.path.clone()),
        names,
        on_clash: i.on_clash,
        kind: i.kind.map(|k| k.code().to_string()),
        units: Some(i.units.to_string()),
        scale: Some(i.scale),
        preview,
        ..Default::default()
    }
}

/// (Re)read the file with the import's options; false when it cannot be read.
fn read(state: &mut AppState, import: &mut ImportDialog) -> bool {
    let out = match state.session.run(command(import, true, None)) {
        Ok(out) => out,
        Err(e) => {
            state.message = e.message;
            return false;
        }
    };
    let picked: Vec<String> = import.entries.iter().filter(|e| !e.pick).map(|e| e.pattern.name.clone()).collect();
    import.entries = out["patterns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let lines: Vec<PatternLine> = serde_json::from_value(p["lines"].clone()).ok()?;
            let kind = FillPatternKind::from_code(p["kind"].as_str()?)?;
            let name = p["name"].as_str()?.to_string();
            let pattern = FillPattern {
                id: FillPatternId(0),
                name: name.clone(),
                description: p["description"].as_str().unwrap_or_default().to_string(),
                kind,
                scale: p["scale"].as_f64().unwrap_or(1.0),
                lines,
            };
            Some(Entry { pattern, exists: p["exists"].as_bool().unwrap_or(false), pick: !picked.contains(&name) })
        })
        .collect();
    import.errors = out["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            let line = e["line"].as_u64().unwrap_or(0);
            let name = e["pattern"].as_str().unwrap_or("—");
            let message = e["message"].as_str().unwrap_or("");
            t!("fp-import-error", "line" => line, "name" => name, "message" => message)
        })
        .collect();
    true
}

fn import_dialog(ctx: &Context, state: &mut AppState) {
    let Some(mut import) = state.dialogs.fill_patterns.as_mut().and_then(|d| d.import.take()) else { return };
    let mut reread = false;
    let mut ok = 0;
    let file = import.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let response = Dialog::new("fill-pattern-import", t!("dlg-fill-pattern-import")).width(620.0).show(
        ctx,
        |ui| {
            ui.label(t!("fp-import-file", "file" => file, "count" => import.entries.len()));
            ui.add_space(4.0);
            egui::Grid::new("fp-import-options").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                row_label(ui, t!("fp-kind"));
                let mut kind = import.kind;
                egui::ComboBox::from_id_salt("fp-import-kind")
                    .selected_text(kind.map_or_else(|| t!("fp-kind-from-file"), |k| tr(k.key())))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut kind, None, t!("fp-kind-from-file"));
                        for k in [FillPatternKind::Drafting, FillPatternKind::Model] {
                            ui.selectable_value(&mut kind, Some(k), tr(k.key()));
                        }
                    });
                reread |= kind != import.kind;
                import.kind = kind;
                ui.end_row();
                row_label(ui, t!("fp-units"));
                let mut units = import.units;
                let unit_label = |u: &str| match u {
                    "mm" => t!("fp-units-mm"),
                    "inch" => t!("fp-units-inch"),
                    _ => t!("fp-units-auto"),
                };
                egui::ComboBox::from_id_salt("fp-import-units").selected_text(unit_label(units)).show_ui(ui, |ui| {
                    for u in ["auto", "mm", "inch"] {
                        ui.selectable_value(&mut units, u, unit_label(u));
                    }
                });
                reread |= units != import.units;
                import.units = units;
                ui.end_row();
                row_label(ui, t!("fp-scale"));
                let r = ui.add(egui::DragValue::new(&mut import.scale).speed(0.01).range(MIN_SCALE..=MAX_SCALE));
                reread |= r.drag_stopped() || (r.changed() && !r.dragged());
                ui.end_row();
                row_label(ui, t!("fp-on-clash"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut import.on_clash, cmd::OnClash::Rename, t!("fp-clash-rename"));
                    ui.radio_value(&mut import.on_clash, cmd::OnClash::Replace, t!("fp-clash-replace"));
                    ui.radio_value(&mut import.on_clash, cmd::OnClash::Skip, t!("fp-clash-skip"));
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.small_button(t!("fp-select-all")).clicked() {
                    import.entries.iter_mut().for_each(|e| e.pick = true);
                }
                if ui.small_button(t!("fp-select-none")).clicked() {
                    import.entries.iter_mut().for_each(|e| e.pick = false);
                }
            });
            egui::ScrollArea::vertical().id_salt("fp-import-list").max_height(320.0).show(ui, |ui| {
                for e in &mut import.entries {
                    ui.horizontal(|ui| {
                        crate::table::checkbox(ui, &mut e.pick);
                        let (rect, _) = ui.allocate_exact_size(vec2(64.0, 32.0), Sense::hover());
                        let painter = ui.painter().with_clip_rect(rect);
                        painter.rect_filled(rect, 1.0, Color32::WHITE);
                        for (a, b) in custom_lines(&e.pattern, rect) {
                            painter.line_segment([a, b], Stroke::new(1.0, Color32::from_gray(40)));
                        }
                        ui.painter().rect_stroke(
                            rect,
                            1.0,
                            Stroke::new(1.0, PALETTE.border_strong),
                            egui::StrokeKind::Inside,
                        );
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.pattern.name).strong());
                            let mut note = tr(e.pattern.kind.key());
                            if !e.pattern.description.is_empty() {
                                note = format!("{note} · {}", e.pattern.description);
                            }
                            if e.exists {
                                note = format!("{note} · {}", t!("fp-exists"));
                            }
                            ui.label(RichText::new(note).size(11.0).color(PALETTE.text_muted));
                        });
                    });
                }
            });
            if !import.errors.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t!("fp-import-problems", "count" => import.errors.len())).color(PALETTE.warning),
                );
                egui::ScrollArea::vertical().id_salt("fp-import-errors").max_height(80.0).show(ui, |ui| {
                    for e in &import.errors {
                        ui.label(RichText::new(e).size(11.0).color(PALETTE.text_muted));
                    }
                });
            }
        },
        |f| {
            ok = f.primary(t!("fp-import-action"));
            f.secondary(t!("action-cancel"));
        },
    );
    if reread {
        read(state, &mut import);
    }
    let done = match response.clicked {
        Some(i) if i == ok => {
            let names: Vec<String> = import.entries.iter().filter(|e| e.pick).map(|e| e.pattern.name.clone()).collect();
            if names.is_empty() {
                state.message = t!("fp-import-none-picked");
                false
            } else if let Some(out) = crate::actions::run_command(state, command(&import, false, Some(names))) {
                let count = |k: &str| out[k].as_array().map_or(0, Vec::len);
                state.message =
                    t!("fp-imported", "count" => count("imported") + count("replaced"), "skipped" => count("skipped"));
                if let Some(id) = out["imported"][0]["id"].as_u64().or_else(|| out["replaced"][0]["id"].as_u64())
                    && let Some(d) = state.dialogs.fill_patterns.as_mut()
                {
                    d.selected = Pattern::Custom(FillPatternId(id));
                }
                true
            } else {
                false
            }
        }
        Some(_) => true,
        None => response.dismissed,
    };
    if !done && let Some(d) = state.dialogs.fill_patterns.as_mut() {
        d.import = Some(import);
    }
}
