//! Files: IFC import/export, SketchUp and OBJ import, sheet PDF/SVG, view PNG/SVG/PDF, schedule CSV, component files.

use super::add_element;
use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::Point;
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::{Session, guard, new_project};
use crate::t;
use cc_core::{Category, Component, Document, ElementKind, ElementType, ElementTypeId, TypeData, ViewKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    Ok(std::fs::write(path, bytes)?)
}

/// Import an IFC file (IFC2X3 or IFC4), replacing the session's project. The
/// result has no file yet; save it with `project_save` and a path.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IfcImport {
    /// Path of the .ifc file.
    pub path: PathBuf,
}

impl Command for IfcImport {
    const NAME: &'static str = "ifc_import";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (project, report) = guard(|| cc_ifc::import_file(&self.path, new_project()))??;
        s.replace(Document::new(project));
        let imported: serde_json::Map<String, Value> =
            report.imported.iter().map(|(c, n)| (refs::category_name(*c), json!(n))).collect();
        Ok(json!({
            "schema": report.schema,
            "levels": report.levels,
            "imported": imported,
            "generic": report.generic,
            "skipped": report.skipped,
        }))
    }
}

/// Export the project as IFC4.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IfcExport {
    /// Path of the .ifc file to write.
    pub path: PathBuf,
}

impl Command for IfcExport {
    const NAME: &'static str = "ifc_export";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        guard(|| cc_ifc::export_to_file(s.project(), &self.path))??;
        Ok(json!({ "path": self.path }))
    }
}

/// Export a sheet as vector PDF or SVG, or as DXF in paper millimeters (by
/// `format`, else by the file extension).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetExport {
    /// Sheet id or name.
    pub sheet: Ref,
    /// Path of the file to write (.pdf, .svg or .dxf).
    pub path: PathBuf,
    /// `pdf`, `svg` or `dxf`.
    #[serde(default)]
    pub format: Option<String>,
}

/// A view's drawing as DXF: a sheet in paper millimeters, any other view at
/// full size in millimeters (texts at their printed size times the scale),
/// without temporary hiding; each element on the layer of its category
/// (detail lines, and regions with a boundary, on the layer of their line
/// style); a view's detail components as blocks and inserts.
fn view_dxf(s: &mut Session, id: cc_core::ViewId) -> Result<Vec<u8>> {
    use cc_views::dxf::{DxfBlocks, DxfScale, default_layer, to_dxf_with};
    let kind = s.project().view(id).map(|v| (matches!(v.kind, ViewKind::Sheet { .. }), v.scale));
    let (drawing, scale, view_scale) = match kind {
        Some((true, _)) => {
            let sheet = s.sheet(id).ok_or_else(|| EngineError::invalid("the sheet cannot be composed"))?;
            (sheet.drawing.clone(), DxfScale::PAPER, None)
        }
        Some((false, scale)) => {
            let drawing =
                s.printed_drawing(id).ok_or_else(|| EngineError::invalid("the view has no drawing to export"))?;
            ((*drawing).clone(), DxfScale::model(scale), Some(scale))
        }
        None => return Err(EngineError::not_found("view")),
    };
    let project = s.project();
    let blocks = match view_scale {
        Some(scale) => {
            cc_views::detail_component::dxf_blocks(project, id, &drawing, scale, &|e| format!("{:?}", e.category()))
        }
        None => DxfBlocks::default(),
    };
    let layer = |prim: &cc_views::Prim| match prim.element().and_then(|e| project.element(e)) {
        // Detail lines go on the layer of their line style.
        Some(cc_core::Element { kind: cc_core::ElementKind::DetailLine(l), .. }) => l.style.code(&project.line_styles),
        // Regions (fill, pattern lines and boundary) on the layer of their boundary's style.
        Some(cc_core::Element {
            kind: cc_core::ElementKind::FilledRegion(r) | cc_core::ElementKind::MaskingRegion(r),
            ..
        }) if r.boundary.is_some() => r.boundary.map(|b| b.code(&project.line_styles)).unwrap_or_default(),
        Some(e) => format!("{:?}", e.category()),
        None => default_layer(prim).to_string(),
    };
    guard(|| to_dxf_with(&drawing, scale, &layer, &blocks))
}

impl Command for SheetExport {
    const NAME: &'static str = "sheet_export";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::view(s.project(), &self.sheet)?;
        if !matches!(s.project().view(id).map(|v| &v.kind), Some(ViewKind::Sheet { .. })) {
            return Err(EngineError::invalid("not a sheet"));
        }
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        let format = self.format.map(|f| f.to_ascii_lowercase()).unwrap_or(ext);
        if format == "dxf" {
            write(&self.path, &view_dxf(s, id)?)?;
            return Ok(json!({ "path": self.path, "format": format }));
        }
        let sheet = s.sheet(id).ok_or_else(|| EngineError::invalid("the sheet cannot be composed"))?;
        let bytes = match format.as_str() {
            "pdf" => guard(|| cc_views::export::to_pdf(&sheet.drawing, sheet.paper))?,
            "svg" => guard(|| cc_views::export::to_svg(&sheet.drawing, sheet.paper))?.into_bytes(),
            other => return Err(EngineError::invalid(format!("unknown format '{other}': use pdf, svg or dxf"))),
        };
        write(&self.path, &bytes)?;
        Ok(json!({ "path": self.path, "format": format }))
    }
}

/// Export any view with a drawing (plan, elevation, section, 3D view as a
/// hidden-line axonometric, legend, drafting view, diagram or sheet) as a
/// PNG image, as vector SVG or PDF, or as DXF for other CAD programs (by
/// `format`, else by the file extension). The view is drawn at its scale as
/// printed, without temporary hiding, on a page that fits it; sheets keep
/// their paper size. A DXF holds the view at full size in millimeters (a
/// sheet in paper millimeters), each element on the layer of its category
/// (detail lines, and regions with a boundary, on the layer named after
/// their line style; region fills as `SOLID` triangles, their patterns as
/// lines), colors as the nearest color index and line patterns as line types.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewExport {
    /// View id or name.
    pub view: Ref,
    /// Path of the file to write (.png, .svg, .pdf or .dxf).
    pub path: PathBuf,
    /// `png`, `svg`, `pdf` or `dxf`.
    #[serde(default)]
    pub format: Option<String>,
    /// Resolution of a PNG, in dots per inch (default 150).
    #[serde(default)]
    pub dpi: Option<f64>,
}

/// Margin around a view's drawing on its page, millimeters.
const VIEW_MARGIN: f64 = 5.0;

impl ViewExport {
    /// The view drawn on paper (millimeters) and the paper size.
    pub fn page(s: &mut Session, view: Ref) -> Result<(cc_views::Drawing2D, glam::DVec2)> {
        let id = refs::view(s.project(), &view)?;
        let kind = s.project().view(id).map(|v| (v.kind.clone(), v.scale));
        match kind {
            Some((ViewKind::Sheet { .. }, _)) => {
                let sheet = s.sheet(id).ok_or_else(|| EngineError::invalid("the sheet cannot be composed"))?;
                Ok((sheet.drawing.clone(), sheet.paper))
            }
            Some((_, scale)) => {
                let drawing =
                    s.printed_drawing(id).ok_or_else(|| EngineError::invalid("the view has no drawing to export"))?;
                Ok(cc_views::export::view_on_paper(&drawing, scale, VIEW_MARGIN))
            }
            None => Err(EngineError::not_found("view")),
        }
    }
}

impl Command for ViewExport {
    const NAME: &'static str = "view_export";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        let format = self.format.map(|f| f.to_ascii_lowercase()).unwrap_or(ext);
        let dpi = refs::finite(self.dpi.unwrap_or(150.0), "dpi")?;
        if format == "dxf" {
            let id = refs::view(s.project(), &self.view)?;
            write(&self.path, &view_dxf(s, id)?)?;
            return Ok(json!({ "path": self.path, "format": format }));
        }
        let (drawing, paper) = Self::page(s, self.view)?;
        let mut out = json!({ "path": self.path, "format": format, "paper_mm": [paper.x, paper.y] });
        let bytes = match format.as_str() {
            "png" => {
                if dpi <= 0.0 {
                    return Err(EngineError::invalid("dpi must be positive"));
                }
                let png = guard(|| cc_views::export::to_png(&drawing, paper, dpi))?.map_err(EngineError::invalid)?;
                let px = (paper / 25.4 * dpi).round();
                out["pixels"] = json!([px.x, px.y]);
                png
            }
            "pdf" => guard(|| cc_views::export::to_pdf(&drawing, paper))?,
            "svg" => guard(|| cc_views::export::to_svg(&drawing, paper))?.into_bytes(),
            other => return Err(EngineError::invalid(format!("unknown format '{other}': use png, svg, pdf or dxf"))),
        };
        write(&self.path, &bytes)?;
        Ok(out)
    }
}

/// Export a schedule view as CSV (UTF-8; semicolons when the decimal comma is used).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleExportCsv {
    /// Schedule view id or name.
    pub view: Ref,
    /// Path of the .csv file to write.
    pub path: PathBuf,
}

impl Command for ScheduleExportCsv {
    const NAME: &'static str = "schedule_export_csv";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::view(s.project(), &self.view)?;
        let table = s.schedule(id).ok_or_else(|| EngineError::invalid("not a schedule view"))?;
        let cell = |c: &cc_views::Cell| match c {
            cc_views::Cell::Text(t) => t.clone(),
            cc_views::Cell::Key(k) => crate::i18n::tr(k),
        };
        // A BOM so spreadsheets read UTF-8 (accents) correctly.
        let csv = format!("\u{feff}{}", cc_views::to_csv(&table, crate::i18n::tr, cell));
        write(&self.path, csv.as_bytes())?;
        Ok(json!({ "path": self.path, "rows": table.rows.len() }))
    }
}

/// Load a component file (.ccomp): its types and missing materials join the project.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentLoad {
    /// Path of the .ccomp file.
    pub path: PathBuf,
}

impl Command for ComponentLoad {
    const NAME: &'static str = "component_load";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.require_files("path")?;
        let file = guard(|| cc_core::component_file::load(&self.path))??;
        let ids = edit(s, t!("tx-load-component"), |e| {
            let wanted = file.types.len() + file.nested.len();
            let mut ids: Vec<u64> = (0..wanted).map(|_| e.alloc_id()).collect();
            ids.reverse();
            let (types, materials) = cc_core::component_file::types_for(e.project(), &file, || ids.pop().unwrap_or(0));
            e.materials_mut().extend(materials);
            Ok(types.into_iter().map(|t| e.add_type(t).0).collect::<Vec<_>>())
        })?;
        Ok(json!({ "component": file.component, "types": ids }))
    }
}

/// Read and parse a SketchUp file.
fn read_skp(path: &Path) -> Result<cc_skp::SkpModel> {
    let file = cc_skp::SkpFile::open(path)?;
    guard(|| file.parse(&Default::default()))?
        .map_err(|e| EngineError::io(t!("err-skp-parse", "detail" => e.to_string())))
}

/// The file name without its extension (the name of what it becomes).
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "SketchUp".into())
}

/// `name`, or `name 2`, `name 3`… when a component type already has it.
fn unique_type_name(p: &cc_core::Project, name: &str, taken: &mut std::collections::HashSet<String>) -> String {
    if taken.is_empty() {
        taken.extend(p.types.values().filter(|t| t.category() == Category::Component).map(|t| t.name.clone()));
    }
    let mut out = name.to_string();
    let mut n = 2;
    while taken.contains(&out) {
        out = format!("{name} {n}");
        n += 1;
    }
    taken.insert(out.clone());
    out
}

/// Add the converted materials (those the project lacks) and types; returns the type ids.
fn add_skp_types(e: &mut cc_core::EditCtx, conv: crate::skp::Converted) -> (Vec<ElementTypeId>, usize) {
    let mut added = 0;
    for m in conv.materials {
        if !e.project().materials.iter().any(|x| x.name == m.name) {
            e.materials_mut().push(m.to_material());
            added += 1;
        }
    }
    let mut taken = std::collections::HashSet::new();
    let mut ids = Vec::with_capacity(conv.types.len());
    for ty in conv.types {
        let name = unique_type_name(e.project(), &ty.name, &mut taken);
        let id = ElementTypeId(e.alloc_id());
        let data = TypeData::Component { component: name.clone(), def: ty.def };
        ids.push(e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() }));
    }
    (ids, added)
}

fn too_large(_: crate::skp::SkpError) -> EngineError {
    EngineError::rejected(t!("err-skp-too-large"))
}

/// Import a SketchUp model (.skp) into the project. Each placed component
/// or group becomes a component element on `level`, of a component type
/// holding its faces as fixed mesh pieces (one per material): placements
/// that differ only by a move or a plan rotation share a type, anything
/// else in their transform (tilt, scale, mirror) is built into the type.
/// Faces outside any component make one more element at the file's origin.
/// Missing materials are added by name, color, opacity and texture; hidden
/// objects and those on hidden layers are left out.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkpImport {
    /// Path of the .skp file.
    pub path: PathBuf,
    /// Level id or name (default: the lowest level); the file's ground is at its elevation.
    #[serde(default)]
    pub level: Option<Ref>,
    /// Where the file's origin goes, in plan (default [0, 0]).
    #[serde(default)]
    pub position: Option<Point>,
    /// Rotation of the whole model in plan, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
}

impl Command for SkpImport {
    const NAME: &'static str = "skp_import";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.require_files("path")?;
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let origin = refs::point(self.position.unwrap_or([0.0, 0.0]))?;
        let turn = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        let model = read_skp(&self.path)?;
        let name = file_stem(&self.path);
        let conv = guard(|| crate::skp::convert(&model, &name))?.map_err(too_large)?;
        if conv.placements.is_empty() {
            return Err(EngineError::rejected(t!("err-skp-empty")));
        }
        let (hidden, faces) = (conv.hidden, conv.faces);
        let placements = conv.placements.clone();
        let rot = glam::DMat2::from_angle(turn.to_radians());
        let (types, elements, materials) = edit(s, t!("tx-import-skp"), |e| {
            let (types, materials) = add_skp_types(e, conv);
            let mut elements = Vec::with_capacity(placements.len());
            for p in &placements {
                let Some(&type_id) = types.get(p.ty) else { continue };
                let component = Component {
                    position: origin + rot * p.position,
                    rotation_deg: p.rotation_deg + turn,
                    base_offset: p.z,
                    overrides: Default::default(),
                    properties: Vec::new(),
                    system: None,
                    ..Default::default()
                };
                elements.push(add_element(e, level, type_id, ElementKind::Component(component)).0);
            }
            Ok((types.iter().map(|t| t.0).collect::<Vec<_>>(), elements, materials))
        })?;
        Ok(json!({ "types": types, "elements": elements, "materials": materials, "faces": faces, "hidden": hidden }))
    }
}

/// Units of the coordinates in an OBJ file (the format does not say).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjUnits {
    #[default]
    M,
    Cm,
    Mm,
    In,
    Ft,
}

impl ObjUnits {
    fn meters(self) -> f64 {
        match self {
            Self::M => 1.0,
            Self::Cm => 0.01,
            Self::Mm => 0.001,
            Self::In => 0.0254,
            Self::Ft => 0.3048,
        }
    }
}

/// Import a Wavefront OBJ model (.obj) into the project. Each object (or,
/// in a file without objects, each group) becomes a component element on
/// `level`, of a component type holding its faces as fixed mesh pieces, one
/// per material, inserted at the center of its footprint and the bottom of
/// its extent. Materials come from the `.mtl` libraries next to the file:
/// color (`Kd`), opacity (`d`, `Tr`) and texture (`map_Kd`, the picture next
/// to the file, repeating as the faces' `vt` coordinates say); missing
/// materials are added by name. Faces not flat are split into triangles;
/// lines, points and normals are ignored.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjImport {
    /// Path of the .obj file.
    pub path: PathBuf,
    /// Level id or name (default: the lowest level); the file's zero height is at its elevation.
    #[serde(default)]
    pub level: Option<Ref>,
    /// Where the file's origin goes, in plan (default [0, 0]).
    #[serde(default)]
    pub position: Option<Point>,
    /// Rotation of the whole model in plan, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Units of the file's coordinates: "m" (default), "cm", "mm", "in" or "ft".
    #[serde(default)]
    pub units: ObjUnits,
    /// The file's Z axis points up; by default Y does (the usual OBJ convention).
    #[serde(default)]
    pub z_up: bool,
    /// Make the whole file one component element, inserted at the file's origin.
    #[serde(default)]
    pub merge: bool,
}

impl Command for ObjImport {
    const NAME: &'static str = "obj_import";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.require_files("path")?;
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let origin = refs::point(self.position.unwrap_or([0.0, 0.0]))?;
        let turn = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        if std::fs::metadata(&self.path)?.len() > crate::obj::MAX_BYTES {
            return Err(EngineError::rejected(t!("err-obj-too-large")));
        }
        let bytes = std::fs::read(&self.path)?;
        let name = file_stem(&self.path);
        let frame = crate::obj::Frame { scale: self.units.meters(), y_up: !self.z_up };
        let dir = self.path.parent();
        let conv = guard(|| crate::obj::convert(&String::from_utf8_lossy(&bytes), dir, frame, &name, self.merge))?
            .map_err(|_| EngineError::rejected(t!("err-obj-too-large")))?;
        if conv.placements.is_empty() {
            return Err(EngineError::rejected(t!("err-obj-empty")));
        }
        let faces = conv.faces;
        let (types, elements, materials) = place_converted(s, conv, level, origin, turn, t!("tx-import-obj"))?;
        Ok(json!({ "types": types, "elements": elements, "materials": materials, "faces": faces }))
    }
}

/// Import a glTF 2.0 model (.glb, or .gltf with its buffers embedded or in
/// files next to it) into the project. Each node of the scene's root, with
/// everything below it, becomes a component element on `level`, of a
/// component type holding its triangles as fixed mesh pieces, one per
/// material, inserted at the center of its footprint and the bottom of its
/// extent. glTF is in meters with Y up. Materials take their base color
/// times its texture's average, the texture itself (repeating as the
/// texture coordinates say) and, when blended, their opacity; missing
/// materials are added by name. Lines and points are ignored, and
/// compressed meshes (Draco, meshopt) are refused.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GltfImport {
    /// Path of the .glb or .gltf file.
    pub path: PathBuf,
    /// Level id or name (default: the lowest level); the file's zero height is at its elevation.
    #[serde(default)]
    pub level: Option<Ref>,
    /// Where the file's origin goes, in plan (default [0, 0]).
    #[serde(default)]
    pub position: Option<Point>,
    /// Rotation of the whole model in plan, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Make the whole scene one component element, inserted at the file's origin.
    #[serde(default)]
    pub merge: bool,
}

impl Command for GltfImport {
    const NAME: &'static str = "gltf_import";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.require_files("path")?;
        use crate::gltf::GltfError;
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let origin = refs::point(self.position.unwrap_or([0.0, 0.0]))?;
        let turn = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        if std::fs::metadata(&self.path)?.len() > crate::gltf::MAX_BYTES {
            return Err(EngineError::rejected(t!("err-gltf-too-large")));
        }
        let bytes = std::fs::read(&self.path)?;
        let name = file_stem(&self.path);
        let dir = self.path.parent();
        let conv = guard(|| crate::gltf::convert(&bytes, dir, &name, self.merge))?.map_err(|e| {
            EngineError::rejected(match e {
                GltfError::Invalid => t!("err-gltf-invalid"),
                GltfError::Unsupported(ext) => t!("err-gltf-compressed", "ext" => ext),
                GltfError::MissingBuffer(uri) => t!("err-gltf-buffer", "uri" => uri),
                GltfError::TooLarge => t!("err-gltf-too-large"),
            })
        })?;
        if conv.placements.is_empty() {
            return Err(EngineError::rejected(t!("err-gltf-empty")));
        }
        let faces = conv.faces;
        let (types, elements, materials) = place_converted(s, conv, level, origin, turn, t!("tx-import-gltf"))?;
        Ok(json!({ "types": types, "elements": elements, "materials": materials, "faces": faces }))
    }
}

/// Add the types of a converted model and an element per placement on
/// `level`, the model moved to `origin` and turned by `turn` degrees, in one
/// undo step named `tx`: the new types, elements and materials.
fn place_converted(
    s: &mut Session,
    conv: crate::skp::Converted,
    level: cc_core::LevelId,
    origin: glam::DVec2,
    turn: f64,
    tx: String,
) -> Result<(Vec<u64>, Vec<u64>, Value)> {
    let placements = conv.placements.clone();
    let rot = glam::DMat2::from_angle(turn.to_radians());
    edit(s, tx, |e| {
        let (types, materials) = add_skp_types(e, conv);
        let mut elements = Vec::with_capacity(placements.len());
        for p in &placements {
            let Some(&type_id) = types.get(p.ty) else { continue };
            let component = Component {
                position: origin + rot * p.position,
                rotation_deg: p.rotation_deg + turn,
                base_offset: p.z,
                ..Default::default()
            };
            elements.push(add_element(e, level, type_id, ElementKind::Component(component)).0);
        }
        Ok((types.iter().map(|t| t.0).collect::<Vec<_>>(), elements, json!(materials)))
    })
}

/// Load a SketchUp model (.skp) as one component type, named after the
/// file, to place like any component: its faces become fixed mesh pieces
/// (one per material) with the file's origin as the insertion point.
/// Missing materials are added by name and color.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkpComponentLoad {
    /// Path of the .skp file.
    pub path: PathBuf,
}

impl Command for SkpComponentLoad {
    const NAME: &'static str = "skp_component_load";
    const KIND: CommandKind = CommandKind::Edit;
    const READS_FILES: bool = true;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.require_files("path")?;
        let model = read_skp(&self.path)?;
        let name = file_stem(&self.path);
        let conv = guard(|| crate::skp::convert_single(&model, &name))?.map_err(too_large)?;
        if conv.types.is_empty() {
            return Err(EngineError::rejected(t!("err-skp-empty")));
        }
        let (types, component) = edit(s, t!("tx-load-component"), |e| {
            let (types, _) = add_skp_types(e, conv);
            let component =
                types.first().and_then(|id| e.project().element_type(*id)).map(|t| t.name.clone()).unwrap_or_default();
            Ok((types.iter().map(|t| t.0).collect::<Vec<_>>(), component))
        })?;
        Ok(json!({ "component": component, "types": types }))
    }
}

/// List the component library: the component files (.ccomp) in `folders`
/// and their subfolders, each with its component, kind, category (doors
/// and windows have their own), types, parameters and connectors; `search`
/// keeps those whose component, type names or descriptions contain it.
/// Files that do not load are listed with the reason.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentLibraryList {
    /// Folders to look in.
    pub folders: Vec<PathBuf>,
    /// Text to look for (any case).
    #[serde(default)]
    pub search: Option<String>,
}

impl Command for ComponentLibraryList {
    const NAME: &'static str = "component_library_list";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, _s: &mut Session) -> Result<Value> {
        let (files, failed) = guard(|| cc_core::component_file::scan(&self.folders))?;
        let search = self.search.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
        let components: Vec<Value> = files
            .iter()
            .filter(|(_, f)| {
                search.as_ref().is_none_or(|q| {
                    let has = |text: &str| text.to_lowercase().contains(q.as_str());
                    has(&f.component) || f.types.iter().any(|t| has(&t.name) || has(&t.info.description))
                })
            })
            .map(|(path, f)| {
                json!({
                    "path": path,
                    "component": f.component,
                    "kind": format!("{:?}", f.def.kind),
                    "category": crate::commands::query::snake(&format!("{:?}", f.def.category())),
                    "types": f.types.iter().map(|t| json!({"name": t.name, "description": t.info.description})).collect::<Vec<_>>(),
                    "parameters": f.def.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    "connectors": f.def.connectors.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                })
            })
            .collect();
        let errors: Vec<Value> = failed.iter().map(|(path, e)| json!({"path": path, "error": e})).collect();
        Ok(json!({ "components": components, "errors": errors }))
    }
}

/// Save a component (all its types and the materials they use) to a .ccomp file.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSave {
    /// Component name (as in a component type's `params.component`).
    pub component: String,
    /// Path of the .ccomp file to write.
    pub path: PathBuf,
}

impl Command for ComponentSave {
    const NAME: &'static str = "component_save";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let file = cc_core::component_file::extract(s.project(), &self.component)?;
        guard(|| cc_core::component_file::save(&file, &self.path))??;
        Ok(json!({ "path": self.path, "types": file.types.len() }))
    }
}
