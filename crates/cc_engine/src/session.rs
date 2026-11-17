use crate::error::{EngineError, ErrorCode, Result};
use crate::t;
use cc_core::{Document, Project, ViewId, ViewKind};
use cc_geom::ModelGeometry;
use cc_views::Drawing2D;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// An open project with its derived geometry and drawings, rebuilt lazily
/// when the document changes.
pub struct Session {
    pub doc: Document,
    /// Merge key for the next edit (see [`Session::run_merged`]).
    merge_key: Option<u64>,
    /// Commands recorded as a macro, while recording.
    recording: Option<Vec<Recorded>>,
    /// The level of the view the user works in (the app's active plan):
    /// recorded steps on it play on the level active when they are played.
    active_level: Option<cc_core::LevelId>,
    geom: ModelGeometry,
    geom_revision: Option<u64>,
    /// Geometry of the phases and design options whose views leave
    /// elements out, with the revision it was built at.
    phase_geoms: HashMap<GeomKey, (u64, ModelGeometry)>,
    drawings: HashMap<ViewId, (u64, Rc<Drawing2D>)>,
    sheets: HashMap<ViewId, (u64, Rc<cc_views::Sheet>)>,
    schedules: HashMap<ViewId, (u64, Rc<cc_views::Table>)>,
    /// Elements temporarily hidden or isolated per view: display state, not
    /// part of the model (not saved, not undone).
    temporary: HashMap<ViewId, cc_core::TemporaryVisibility>,
    /// Counts changes of `temporary` (3D views rebuild their meshes on it).
    temporary_revision: u64,
    /// Commands added at run time (plugins), kept when the document is replaced.
    extension: Option<Box<dyn crate::Extension>>,
    /// How many commands are running (a plugin command runs others).
    nesting: u32,
    /// Whether the commands running may work with files (see
    /// [`Session::with_files`]).
    files: bool,
}

/// What a view's own geometry depends on: its phase (an index, when it
/// leaves elements out) and the design options it shows.
type GeomKey = (Option<usize>, std::collections::BTreeSet<cc_core::OptionId>);

/// A command recorded in a macro.
#[derive(Debug, Clone)]
struct Recorded {
    step: crate::script::Step,
    /// The ids it allocated and its result, to refer to from later steps.
    made: Option<crate::script::Made>,
    merge_key: Option<u64>,
    /// Undo depth before the command ran.
    depth: usize,
}

/// A new project from the metric template, named in the active language.
pub fn new_project() -> Project {
    new_project_with(cc_core::MeasurementSystem::Metric)
}

/// A new project from the template of `system`, named in the active language.
pub fn new_project_with(system: cc_core::MeasurementSystem) -> Project {
    Project::from_template_system(system, &crate::i18n::try_tr)
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Document::new(new_project()))
    }
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            merge_key: None,
            recording: None,
            active_level: None,
            geom: ModelGeometry::default(),
            geom_revision: None,
            phase_geoms: HashMap::new(),
            drawings: HashMap::new(),
            sheets: HashMap::new(),
            schedules: HashMap::new(),
            temporary: HashMap::new(),
            temporary_revision: 0,
            extension: None,
            nesting: 0,
            files: true,
        }
    }

    /// Open a `.ccad` file; returns the session and the number of repairs made.
    pub fn open(path: &std::path::Path) -> Result<(Self, usize)> {
        let (project, repairs) = guard(|| cc_core::load_with_repairs_localized(path, &crate::i18n::try_tr))??;
        let mut doc = Document::new(project);
        doc.path = Some(path.to_path_buf());
        Ok((Self::new(doc), repairs))
    }

    /// Replace the document (new, open, import), dropping derived data.
    pub fn replace(&mut self, doc: Document) {
        let extension = self.extension.take();
        let nesting = self.nesting;
        let files = self.files;
        *self = Self::new(doc);
        self.extension = extension;
        self.nesting = nesting;
        self.files = files;
    }

    pub fn project(&self) -> &Project {
        &self.doc.project
    }

    /// Format quantities with the project's units (drawings, schedules).
    fn sync_units(&self) {
        cc_views::format::set_units(&self.doc.project.units);
    }

    /// Rebuild derived geometry if the document changed; returns whether it did.
    pub fn sync(&mut self) -> bool {
        let rev = self.doc.revision();
        if self.geom_revision == Some(rev) {
            return false;
        }
        self.geom = ModelGeometry::build(&self.doc.project);
        self.geom_revision = Some(rev);
        self.phase_geoms.retain(|_, (r, _)| *r == rev);
        // Drop what is kept for views that are gone.
        let views = &self.doc.project.views;
        self.drawings.retain(|v, _| views.contains_key(v));
        self.sheets.retain(|v, _| views.contains_key(v));
        self.schedules.retain(|v, _| views.contains_key(v));
        true
    }

    /// The phase (an index, when it leaves elements out) and design options
    /// whose own geometry `view` shows, or `None` when it shows the whole
    /// model's (see [`ModelGeometry::build_view`]).
    fn geom_key(&self, view: ViewId) -> Option<GeomKey> {
        let p = &self.doc.project;
        let v = p.view(view)?;
        if !v.kind.is_phased() {
            return None;
        }
        let phase = p.view_phase(v).and_then(|ph| p.phase_index(ph)).filter(|i| p.phase_differs(*i));
        let options = p.shown_options(Some(v));
        if phase.is_none() && options == p.shown_options(None) {
            return None;
        }
        Some((phase, options))
    }

    /// Build (if needed) the geometry `view` shows; returns its key.
    fn sync_view(&mut self, view: ViewId) -> Option<GeomKey> {
        self.sync();
        let key = self.geom_key(view)?;
        let rev = self.doc.revision();
        if self.phase_geoms.get(&key).is_none_or(|(r, _)| *r != rev) {
            let g = ModelGeometry::build_view(&self.doc.project, key.0, &key.1, &self.geom);
            self.phase_geoms.insert(key.clone(), (rev, g));
        }
        Some(key)
    }

    fn geom_of(&self, key: Option<GeomKey>) -> &ModelGeometry {
        key.and_then(|k| self.phase_geoms.get(&k)).map_or(&self.geom, |(_, g)| g)
    }

    /// Geometry of the model as `view` shows it, in its phase: only the
    /// elements of that phase join, open walls and bound rooms. The whole
    /// model's geometry when the phase leaves nothing out.
    pub fn view_geometry(&mut self, view: ViewId) -> &ModelGeometry {
        let key = self.sync_view(view);
        self.geom_of(key)
    }

    /// [`Session::view_geometry`] as of the last sync (the GUI calls
    /// [`Session::view_geometry`] once a frame per open view first); falls
    /// back to the whole model's geometry when the phase's is not built.
    pub fn geom_for(&self, view: ViewId) -> &ModelGeometry {
        let rev = self.doc.revision();
        match self.geom_key(view).and_then(|k| self.phase_geoms.get(&k)) {
            Some((r, g)) if *r == rev => g,
            _ => &self.geom,
        }
    }

    /// Geometry of the model in which element `id` stands: the whole
    /// model's, or, for an element of a design option views do not show by
    /// default, the main model with that option (cached like a view's).
    pub fn element_geometry(&mut self, id: cc_core::ElementId) -> &ModelGeometry {
        self.sync();
        let p = &self.doc.project;
        let mut options = p.shown_options(None);
        let key = match p.element(id).and_then(|e| e.design_option) {
            Some(o) if !options.contains(&o) => {
                if let Some(set) = p.option_set_of(o) {
                    options.retain(|x| !set.contains(*x));
                }
                options.insert(o);
                Some((None, options))
            }
            _ => None,
        };
        let rev = self.doc.revision();
        if let Some(key) = &key
            && self.phase_geoms.get(key).is_none_or(|(r, _)| *r != rev)
        {
            let g = ModelGeometry::build_view(&self.doc.project, None, &key.1, &self.geom);
            self.phase_geoms.insert(key.clone(), (rev, g));
        }
        self.geom_of(key)
    }

    /// Derived geometry as of the last [`Session::sync`] (callers that keep
    /// it current, like the GUI once a frame, read it without `&mut`).
    pub fn geom(&self) -> &ModelGeometry {
        &self.geom
    }

    /// Derived geometry of the current revision.
    pub fn geometry(&mut self) -> &ModelGeometry {
        self.sync();
        &self.geom
    }

    /// The project with its current geometry.
    pub fn project_and_geometry(&mut self) -> (&Project, &ModelGeometry) {
        self.sync();
        (&self.doc.project, &self.geom)
    }

    /// Drawing of a graphical view, with what is temporarily hidden or
    /// isolated in it left out (cached per revision).
    pub fn drawing(&mut self, view: ViewId) -> Option<Rc<Drawing2D>> {
        let rev = self.doc.revision();
        if let Some((r, d)) = self.drawings.get(&view)
            && *r == rev
        {
            return Some(d.clone());
        }
        self.sync_units();
        let key = self.sync_view(view);
        let v = self.doc.project.view(view)?;
        let temporary = self.temporary.get(&view);
        let geom = self.geom_of(key);
        let d = Rc::new(cc_views::generate_temporary(&self.doc.project, geom, v, &crate::i18n::tr, temporary)?);
        self.drawings.insert(view, (rev, d.clone()));
        Some(d)
    }

    /// Drawing of a view as printed: without temporary hiding or isolation,
    /// and without its underlay unless the view prints it.
    pub fn printed_drawing(&mut self, view: ViewId) -> Option<Rc<Drawing2D>> {
        let d = if self.temporary.contains_key(&view) {
            self.sync_units();
            let key = self.sync_view(view);
            let v = self.doc.project.view(view)?;
            Rc::new(cc_views::generate_labeled(&self.doc.project, self.geom_of(key), v, &crate::i18n::tr)?)
        } else {
            self.drawing(view)?
        };
        let printed = self.doc.project.view(view).and_then(|v| v.kind.underlay()).is_some_and(|u| u.printed);
        Some(if d.underlay > 0 && !printed { Rc::new(d.without_underlay()) } else { d })
    }

    /// What is temporarily hidden or isolated in `view`.
    pub fn temporary(&self, view: ViewId) -> Option<&cc_core::TemporaryVisibility> {
        self.temporary.get(&view)
    }

    /// Set (or, with `None` or an empty set, reset) what is temporarily
    /// hidden or isolated in `view`. Not an edit: nothing to undo or save.
    pub fn set_temporary(&mut self, view: ViewId, temporary: Option<cc_core::TemporaryVisibility>) {
        match temporary.filter(|t| !t.is_empty()) {
            Some(t) => self.temporary.insert(view, t),
            None => self.temporary.remove(&view),
        };
        self.drawings.remove(&view);
        self.temporary_revision += 1;
    }

    /// Keep the camera of 3D view `view` (not undoable). The model's geometry
    /// stays: only the view's drawing and the sheets showing it are redrawn.
    pub fn set_view_camera(&mut self, view: ViewId, camera: cc_core::CameraState) {
        self.doc.set_view_state(|p| {
            if let Some(ViewKind::ThreeD { camera: c, .. }) = p.views.get_mut(&view).map(|v| &mut v.kind) {
                *c = camera;
            }
        });
        self.drawings.remove(&view);
        let project = &self.doc.project;
        self.sheets.retain(|sheet, _| {
            !matches!(project.view(*sheet).map(|v| &v.kind),
                Some(ViewKind::Sheet { viewports, .. }) if viewports.iter().any(|vp| vp.view == view))
        });
    }

    /// Changes whenever any view's temporary visibility changes.
    pub fn temporary_revision(&self) -> u64 {
        self.temporary_revision
    }

    /// Drop cached drawings (after a language change alters their text).
    pub fn invalidate_drawings(&mut self) {
        self.drawings.clear();
        self.sheets.clear();
        self.schedules.clear();
    }

    /// Composition of a sheet in paper millimeters (cached per revision).
    pub fn sheet(&mut self, view: ViewId) -> Option<Rc<cc_views::Sheet>> {
        let rev = self.doc.revision();
        if let Some((r, s)) = self.sheets.get(&view)
            && *r == rev
        {
            return Some(s.clone());
        }
        let sheet_view = self.doc.project.view(view)?.clone();
        let env = cc_views::SheetEnv {
            today: crate::i18n::today(),
            format_date: crate::i18n::format_date,
            as_indicated: t!("sheet-as-indicated"),
        };
        let ViewKind::Sheet { viewports, .. } = &sheet_view.kind else { return None };
        let drawings: HashMap<ViewId, Rc<Drawing2D>> =
            viewports.iter().filter_map(|vp| Some((vp.view, self.printed_drawing(vp.view)?))).collect();
        let s = Rc::new(cc_views::compose_sheet(&self.doc.project, &sheet_view, &env, |v| drawings.get(&v).cloned())?);
        self.sheets.insert(view, (rev, s.clone()));
        Some(s)
    }

    /// A schedule table of a schedule view, with the project's units: the
    /// elements its phase and phase filter show (cached per revision).
    pub fn schedule(&mut self, view: ViewId) -> Option<Rc<cc_views::Table>> {
        let rev = self.doc.revision();
        if let Some((r, t)) = self.schedules.get(&view)
            && *r == rev
        {
            return Some(t.clone());
        }
        let (category, def) = match &self.doc.project.view(view)?.kind {
            ViewKind::Schedule { category, def } => (*category, def.clone()),
            _ => return None,
        };
        self.sync_units();
        self.geometry();
        let p = &self.doc.project;
        let v = p.view(view)?;
        let phase = cc_views::graphics::PhaseView::of(p, v);
        let options = p.shown_options(Some(v));
        let table = Rc::new(cc_views::schedule_where(p, &self.geom, category, &def, &mut |e| {
            p.option_shows(e, &options) && phase.is_none_or(|ph| ph.shows(p, e))
        }));
        self.schedules.insert(view, (rev, table.clone()));
        Some(table)
    }

    /// Run a typed command (the way the GUI calls the engine): it goes
    /// through [`Session::execute`] like a command from the CLI or MCP.
    pub fn run<C: crate::Command + serde::Serialize>(&mut self, command: C) -> Result<Value> {
        let args = serde_json::to_value(&command).map_err(|e| EngineError::invalid(e.to_string()))?;
        self.execute(C::NAME, args)
    }

    /// Run a typed command whose edit merges with the previous one of the
    /// same `key` into one undo step (a value being dragged); a different
    /// key, an unmerged edit or [`Document::break_merge`] starts a new step.
    pub fn run_merged<C: crate::Command + serde::Serialize>(&mut self, key: u64, command: C) -> Result<Value> {
        self.merge_key = Some(key);
        let result = self.run(command);
        self.merge_key = None;
        result
    }

    /// The merge key for the edit being run.
    pub(crate) fn take_merge_key(&mut self) -> Option<u64> {
        self.merge_key.take()
    }

    /// Start recording the commands that change the model (a macro).
    pub fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording; returns the recorded steps (a dragged value counts once,
    /// with its final value). Ids of objects an earlier step created become
    /// `{"$step": n}` references and the active level `{"$level": "active"}`
    /// (see [`crate::script`]), so the macro plays on any level and project.
    pub fn stop_recording(&mut self) -> Vec<crate::script::Step> {
        let (mut steps, made): (Vec<_>, Vec<_>) =
            self.recording.take().unwrap_or_default().into_iter().map(|r| (r.step, r.made)).unzip();
        crate::script::link(&mut steps, &made);
        steps
    }

    /// Set the level of the view the user works in (`None` outside plans);
    /// see [`Session::active_level`].
    pub fn set_active_level(&mut self, level: Option<cc_core::LevelId>) {
        self.active_level = level;
    }

    /// A level by id or name (`ccad script level=…`).
    pub fn level(&self, r: &crate::Ref) -> Result<cc_core::LevelId> {
        crate::refs::level(&self.doc.project, r)
    }

    /// The level of the view the user works in, if it exists: recording
    /// writes it as the active level placeholder, and scripts play that
    /// placeholder on it.
    pub fn active_level(&self) -> Option<cc_core::LevelId> {
        self.active_level.filter(|l| self.doc.project.level(*l).is_some())
    }

    /// Undo back to undo depth `depth` (cancelling a dialog), dropping what
    /// was recorded since.
    pub fn undo_to(&mut self, depth: usize) {
        while self.doc.undo_depth() > depth && self.doc.undo() {}
        if let Some(rec) = &mut self.recording {
            rec.retain(|r| r.depth < depth);
        }
    }

    /// Record a command that succeeded; `first_id` is the id counter before
    /// it ran.
    fn record(&mut self, name: &str, args: Value, result: &Value, first_id: u64, merge_key: Option<u64>, depth: usize) {
        if self.recording.is_none() {
            return;
        }
        let kind = self.kind_of(name);
        // Edits only: replaying a macro should not open, save or export files.
        if kind != Some(crate::CommandKind::Edit) {
            return;
        }
        let mut args = args;
        if let Some(level) = self.active_level().and_then(|l| self.doc.project.level(l)) {
            crate::script::mark_level(&mut args, level.id, &level.name, 0);
        }
        let next_id = self.doc.project.next_id();
        let made = (next_id > first_id).then(|| crate::script::Made { ids: first_id..next_id, result: result.clone() });
        let Some(rec) = &mut self.recording else { return };
        let step = crate::script::Step { command: name.to_string(), args };
        match rec.last_mut() {
            // A value being dragged: keep its last state only.
            Some(last) if merge_key.is_some() && last.merge_key == merge_key && last.step.command == name => {
                last.step = step;
                if let Some(m) = made {
                    last.made = Some(match last.made.take() {
                        Some(old) => crate::script::Made { ids: old.ids.start..m.ids.end, result: m.result },
                        None => m,
                    });
                }
            }
            _ => rec.push(Recorded { step, made, merge_key, depth }),
        }
    }

    /// Set the source of commands beyond the built-in catalog (plugins), or
    /// remove it with `None`; returns the one it replaces.
    pub fn set_extension(&mut self, extension: Option<Box<dyn crate::Extension>>) -> Option<Box<dyn crate::Extension>> {
        std::mem::replace(&mut self.extension, extension)
    }

    pub fn extension(&self) -> Option<&dyn crate::Extension> {
        self.extension.as_deref()
    }

    /// The extension's messages for the user since the last call.
    pub fn take_extension_messages(&mut self) -> Vec<String> {
        self.extension.as_mut().map(|e| e.take_messages()).unwrap_or_default()
    }

    /// Whether the command running was called by another command (a plugin
    /// command), not by the user.
    pub fn called_by_command(&self) -> bool {
        self.nesting > 1
    }

    /// Whether a command is running (a plugin command running others).
    pub fn is_nested(&self) -> bool {
        self.nesting > 0
    }

    /// Whether the commands running may read and write files: true unless a
    /// caller without file access (a plugin without the `files` permission,
    /// a graph run without `allow_files`) runs them.
    pub fn files_allowed(&self) -> bool {
        self.files
    }

    /// Run `f` with file access narrowed to `allowed` (it is never widened:
    /// a caller without files cannot give them to what it runs).
    pub fn with_files<R>(&mut self, allowed: bool, f: impl FnOnce(&mut Session) -> R) -> R {
        let outer = self.files;
        self.files = outer && allowed;
        let result = f(self);
        self.files = outer;
        result
    }

    /// Refuse a file path given to a command (in its parameter `param`) when
    /// the commands running may not work with files.
    pub fn require_files(&self, param: &str) -> Result<()> {
        if self.files {
            Ok(())
        } else {
            Err(EngineError::rejected(format!(
                "`{param}` names a file, and this run may not work with files (a plugin needs the \"files\" \
                 permission, a graph `allow_files`)"
            )))
        }
    }

    /// Every command this session runs: the built-in catalog, then the
    /// extension's commands.
    pub fn commands(&self) -> Vec<crate::CommandInfo> {
        let mut all = crate::catalog();
        if let Some(ext) = &self.extension {
            all.extend(ext.commands());
        }
        all
    }

    /// The catalog entry of one command, built-in or from the extension.
    pub fn describe(&self, name: &str) -> Option<crate::CommandInfo> {
        if crate::extension::is_extension_name(name) {
            self.extension.as_ref()?.commands().into_iter().find(|c| c.name == name)
        } else {
            crate::describe(name)
        }
    }

    /// The kind of a command, built-in or from the extension.
    pub fn kind_of(&self, name: &str) -> Option<crate::CommandKind> {
        if crate::extension::is_extension_name(name) {
            self.extension.as_ref()?.commands().into_iter().find(|c| c.name == name).map(|c| c.kind)
        } else {
            crate::command_kind(name)
        }
    }

    /// Run the command `name` with JSON `args`. A panic inside the command
    /// becomes an internal error and the document is kept. Commands of the
    /// extension (names with `__`) are one undo step however many commands
    /// they run, and leave nothing behind when they fail.
    pub fn execute(&mut self, name: &str, args: Value) -> Result<Value> {
        let depth = self.doc.undo_depth();
        let merge_key = self.merge_key;
        // Commands run by another command are part of it, not steps of a macro.
        let recorded_args = (self.recording.is_some() && self.nesting == 0).then(|| args.clone());
        let first_id = self.doc.project.next_id();
        if !self.files && self.kind_of(name) == Some(crate::CommandKind::File) {
            return Err(EngineError::rejected(format!(
                "{name} works with files, and this run may not (a plugin needs the \"files\" permission, a graph \
                 `allow_files`)"
            )));
        }
        let outer = self.nesting;
        self.nesting = outer.saturating_add(1);
        // Names and messages a command writes follow the project's units.
        self.sync_units();
        let result = if crate::extension::is_extension_name(name) {
            self.execute_extension(name, args, depth)
        } else {
            guard(|| crate::registry::dispatch(self, name, args)).and_then(|r| r)
        };
        self.nesting = outer;
        if let (Ok(out), Some(args)) = (&result, recorded_args) {
            self.record(name, args, out, first_id, merge_key, depth);
        }
        if let Err(EngineError { code: ErrorCode::Internal, .. }) = &result {
            // Undo whatever the failed command left behind.
            while self.doc.undo_depth() > depth && self.doc.undo() {}
        }
        result
    }

    fn execute_extension(&mut self, name: &str, args: Value, depth: usize) -> Result<Value> {
        let Some(mut ext) = self.extension.take() else {
            let message = if self.nesting > 1 {
                format!("{name}: a plugin command cannot run inside another plugin command")
            } else {
                format!("unknown command: {name}")
            };
            return Err(EngineError::new(ErrorCode::UnknownCommand, message));
        };
        // Its edits are squashed into one step: none merges with an earlier one.
        self.merge_key = None;
        self.doc.break_merge();
        let result = guard(|| ext.run(self, name, args));
        if self.extension.is_none() {
            self.extension = Some(ext);
        }
        let result = match result {
            Ok(Some(r)) => r,
            Ok(None) => Err(EngineError::new(ErrorCode::UnknownCommand, format!("unknown command: {name}"))),
            Err(e) => Err(e),
        };
        match &result {
            Ok(_) => {
                self.doc.squash_since(depth, name);
            }
            Err(_) => self.undo_to(depth),
        }
        result
    }
}

/// Last-resort guard: a panic becomes an internal error.
pub(crate) fn guard<T>(f: impl FnOnce() -> T) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        EngineError::new(ErrorCode::Internal, format!("internal error: {detail}"))
    })
}
