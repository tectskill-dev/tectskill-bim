//! ConstructCAD — open source BIM authoring.

// Release builds on Windows open no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod browser;
mod canvas;
mod clipboard;
mod commands;
mod component_editor;
mod dialog;
mod dialogs;
mod dim_edit;
mod engine_edit;
mod graphics_dialog;
mod home;
mod i18n;
mod icons;
mod library;
mod material_browser;
mod modify;
mod modify3d;
mod paint;
mod palette;
mod phases_dialog;
mod properties;
mod recovery;
mod ribbon;
mod schedule_ui;
mod sheet;
mod stair_ui;
mod state;
mod systems_dialog;
mod table;
mod text_tool;
mod theme;
mod thumbnails;
mod tools;
mod type_dialog;
#[cfg(test)]
mod ui_tests;
mod units_dialog;
mod update;
mod view3d;
mod view_cube;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    // Panics are logged to the crash log (release builds on Windows have no console).
    recovery::install_panic_hook();
    let args = parse_args(std::env::args().skip(1));
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("TectSkill BIM")
        .with_icon(app_icon());
    // The Wayland app id / X11 class names the desktop file (packaging/linux),
    // so desktops show its icon. eframe keeps its settings under the same
    // (lowercased) folder name on Linux, so nothing moves.
    #[cfg(target_os = "linux")]
    let viewport = viewport.with_app_id("tectskill-bim");
    let options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Wgpu, ..Default::default() };
    eframe::run_native("TectSkill BIM", options, Box::new(|cc| Ok(Box::new(app::ConstructApp::new(cc, args)))))
}

/// Window and taskbar icon.
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(theme::APP_ICON_PNG).unwrap_or_default()
}

/// `tectskill-bim [project.ccad] [--view NAME]... [--lang en|pt-BR|es|fr|de|it|lo|pl|th|uk-UA|zh-TW]`
///
/// Named views replace the default layout (the first floor plan beside the
/// 3D view): the first on the left, the second on the right, others as tabs.
fn parse_args(args: impl Iterator<Item = String>) -> app::Args {
    let mut out = app::Args::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--view" => out.views.extend(args.next()),
            "--lang" => out.language = args.next().and_then(|c| i18n::Language::from_code(&c)),
            _ if out.path.is_none() => out.path = Some(std::path::PathBuf::from(arg)),
            _ => {}
        }
    }
    out
}
