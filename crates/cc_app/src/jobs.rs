//! Long commands (opening a project, importing IFC, DWG/DXF, SketchUp,
//! pictures, component files) run on a worker thread over a copy of the
//! document, while a modal dialog shows what is being read and for how long.
//! The app keeps drawing the document as it was; when the command ends, its
//! document comes back to the UI thread, which installs it and reports.
//!
//! Only one job runs at a time, and while it runs the dialog blocks the
//! interface and [`busy`] keeps keyboard shortcuts and dropped files out, so
//! nothing edits the document the job's copy will replace.

use crate::dialog::Dialog;
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::Document;
use cc_engine::{EngineError, ErrorCode, Session};
use egui::{Context, RichText};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// What the command runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// A new document (opening a project, importing an IFC model): the
    /// result replaces the open one, as the finish step decides.
    Fresh,
    /// A copy of the open document, which the result replaces (with its
    /// undo history and the command's undo step) when the command succeeds.
    Current,
}

/// What the worker sends back: the document it ran on, and the command's answer.
type Outcome = Result<(Document, Value), EngineError>;

/// The step that runs on the UI thread when the command ends: the answer,
/// and for a [`Base::Fresh`] job that succeeded, the new document.
pub type Finish = Box<dyn FnOnce(&mut AppState, Result<Value, EngineError>, Option<Document>)>;

/// A running job.
pub struct Job {
    /// The dialog's title (*Importing IFC*…).
    title: String,
    path: PathBuf,
    /// The file's size in bytes, when it could be read.
    size: Option<u64>,
    started: Instant,
    base: Base,
    rx: Receiver<Outcome>,
    finish: Option<Finish>,
}

/// A job is running: shortcuts, dropped files and new jobs wait.
pub fn busy(state: &AppState) -> bool {
    state.job.is_some()
}

/// Run `run` on a worker thread over the document `base` names, showing the
/// progress dialog titled `title` for `path`; `finish` reports the result.
pub fn start(
    state: &mut AppState,
    title: String,
    path: PathBuf,
    base: Base,
    run: impl FnOnce(&mut Session) -> Result<Value, EngineError> + Send + 'static,
    finish: Finish,
) {
    if busy(state) {
        state.message = t!("job-busy");
        return;
    }
    // Built here: the template's names follow the interface language.
    let doc = match base {
        Base::Fresh => Document::new(cc_engine::new_project()),
        Base::Current => state.session.doc.clone(),
    };
    let (tx, rx) = mpsc::channel();
    let work = move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut session = Session::new(doc);
            let answer = run(&mut session);
            answer.map(|value| (session.doc, value))
        }))
        .unwrap_or_else(|_| Err(EngineError::new(ErrorCode::Internal, t!("job-failed"))));
        // The job was cancelled when nobody listens any more.
        let _ = tx.send(outcome);
    };
    if let Err(e) = std::thread::Builder::new().name("constructcraft-job".into()).spawn(work) {
        finish(state, Err(EngineError::new(ErrorCode::Internal, e.to_string())), None);
        return;
    }
    let size = std::fs::metadata(&path).ok().map(|m| m.len());
    state.job = Some(Job { title, path, size, started: Instant::now(), base, rx, finish: Some(finish) });
}

/// Install the result of a job that ended; call it once per frame.
pub fn poll(state: &mut AppState) {
    let Some(job) = state.job.as_mut() else { return };
    let outcome = match job.rx.try_recv() {
        Ok(outcome) => outcome,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err(EngineError::new(ErrorCode::Internal, t!("job-failed"))),
    };
    if let Some(job) = state.job.take() {
        complete(state, job, outcome);
    }
}

/// Wait for the running job and install its result (at startup, before the
/// window shows, and in tests).
pub fn wait(state: &mut AppState) {
    let Some(job) = state.job.take() else { return };
    let outcome = job.rx.recv().unwrap_or_else(|_| Err(EngineError::new(ErrorCode::Internal, t!("job-failed"))));
    complete(state, job, outcome);
}

fn complete(state: &mut AppState, mut job: Job, outcome: Outcome) {
    let Some(finish) = job.finish.take() else { return };
    match (outcome, job.base) {
        (Ok((doc, value)), Base::Current) => {
            state.session.doc = doc;
            finish(state, Ok(value), None);
        }
        (Ok((doc, value)), Base::Fresh) => finish(state, Ok(value), Some(doc)),
        (Err(e), _) => finish(state, Err(e), None),
    }
}

/// Stop waiting for the running job: its result is dropped and the document stays as it was.
pub fn cancel(state: &mut AppState) {
    if state.job.take().is_some() {
        state.message = t!("job-cancelled");
    }
}

/// A size in bytes for people (KB, MB, GB with one decimal).
fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

/// Elapsed time as minutes and seconds.
fn elapsed_text(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The progress dialog of the running job.
pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(job) = &state.job else { return };
    let name = job.path.file_name().map_or_else(|| job.path.display().to_string(), |n| n.to_string_lossy().into());
    let folder = job.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let details = match job.size {
        Some(bytes) => t!("job-details", "size" => size_text(bytes), "time" => elapsed_text(job.started.elapsed())),
        None => t!("job-elapsed", "time" => elapsed_text(job.started.elapsed())),
    };
    let mut cancel_button = 0;
    let response = Dialog::new("job", job.title.clone()).width(380.0).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(32.0).color(PALETTE.accent));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(theme::title(t!("job-reading", "name" => name)));
                    if !folder.is_empty() {
                        ui.add(egui::Label::new(theme::muted(folder)).truncate());
                    }
                    ui.label(RichText::new(details).size(12.0).color(PALETTE.text_faint));
                });
            });
            ui.add_space(10.0);
            ui.label(RichText::new(t!("job-hint")).size(11.5).color(PALETTE.text_muted));
        },
        |footer| cancel_button = footer.secondary(t!("action-cancel")),
    );
    // The elapsed time moves on even when nothing else does.
    ctx.request_repaint_after(Duration::from_millis(250));
    if response.dismissed || response.clicked == Some(cancel_button) {
        cancel(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_times_read_for_people() {
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(2048), "2.0 KB");
        assert_eq!(size_text(5 * 1024 * 1024 + 300 * 1024), "5.3 MB");
        assert_eq!(elapsed_text(Duration::from_secs(75)), "1:15");
    }

    /// A job on the open document replaces it with its result (one undo
    /// step), a failed one leaves it as it was, and a cancelled one's result
    /// is dropped.
    #[test]
    fn jobs_install_their_result_or_leave_the_document() {
        let mut state = AppState::new(Document::new(cc_engine::new_project()));
        let walls =
            |s: &AppState| s.project().elements.values().filter(|e| e.category() == cc_core::Category::Wall).count();
        let before = walls(&state);
        let wall =
            |s: &mut Session| s.execute("wall_create", serde_json::json!({"start": [0.0, 0.0], "end": [4.0, 0.0]}));
        let done: Finish = Box::new(|state, result, doc| {
            assert!(doc.is_none());
            state.message = format!("{}", result.is_ok());
        });
        start(&mut state, "t".into(), PathBuf::from("x.dwg"), Base::Current, wall, done);
        assert!(busy(&state));
        wait(&mut state);
        assert_eq!((walls(&state), state.message.as_str()), (before + 1, "true"));
        assert!(state.session.doc.can_undo(), "the command's undo step came back");

        let fail = |s: &mut Session| s.execute("no_such_command", serde_json::json!({}));
        start(
            &mut state,
            "t".into(),
            PathBuf::from("x"),
            Base::Current,
            fail,
            Box::new(|s, r, _| s.message = format!("{}", r.is_ok())),
        );
        wait(&mut state);
        assert_eq!((walls(&state), state.message.as_str()), (before + 1, "false"));

        start(&mut state, "t".into(), PathBuf::from("x"), Base::Current, wall, Box::new(|_, _, _| panic!("cancelled")));
        cancel(&mut state);
        assert!(!busy(&state));
        assert_eq!(walls(&state), before + 1);
    }
}
