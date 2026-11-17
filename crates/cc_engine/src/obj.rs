//! Wavefront OBJ models as component types: each object (`o`, or `g` in a
//! file without objects) becomes a type of fixed mesh pieces, one per
//! material of its `.mtl` library, placed by an element at the center of
//! its footprint and the bottom of its extent. OBJ has no units nor a fixed
//! up axis, so the caller gives both. Materials take `Kd`, `d`/`Tr` and the
//! picture of `map_Kd` (by file name next to the model), its repeat from
//! the faces' texture coordinates (`vt`, see [`crate::import_material`]).
//!
//! Files are hostile input: unknown statements are skipped, faces with a
//! bad index are dropped, and the sizes read are bounded.

use crate::import_material::{UvScale, to_linear};
use crate::skp::{Converted, MeshBuild, SkpPlacement, SkpType, mesh_definition};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Largest .obj file read, in bytes.
pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Largest .mtl file read, in bytes.
const MAX_MTL_BYTES: u64 = 16 * 1024 * 1024;
/// Vertices read from a file.
const MAX_VERTICES: usize = 10_000_000;
/// Corners of one face.
const MAX_CORNERS: usize = 4_096;
/// Objects (types) an import makes.
const MAX_OBJECTS: usize = 20_000;
/// Faces bent more than this (in meters) out of their plane are split into triangles.
const PLANE_TOLERANCE: f64 = 1e-6;

/// How the file's coordinates map to the model's.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Meters per file unit.
    pub scale: f64,
    /// The file's Y axis points up (the usual OBJ convention); else Z does.
    pub y_up: bool,
}

#[derive(Debug)]
pub enum ObjError {
    /// More vertices, faces or objects than an import takes.
    TooLarge,
}

/// A face: its material, vertex indices and texture coordinate indices
/// (empty when not every corner has one).
type ObjFace = (Option<usize>, Vec<u32>, Vec<u32>);

/// One object of the file: its faces.
struct Object {
    name: String,
    faces: Vec<ObjFace>,
}

/// The library statements and material uses of a file, before materials are read.
struct Parsed {
    vertices: Vec<Option<DVec3>>,
    /// Texture coordinates (`vt`).
    uvs: Vec<Option<DVec2>>,
    objects: Vec<Object>,
    libraries: Vec<String>,
    /// Material names in the order first used.
    used: Vec<String>,
}

/// Read the statements of `text` (with `\` line continuations).
fn statements(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut lines = text.lines();
    std::iter::from_fn(move || {
        let mut out = String::new();
        loop {
            let line = lines.next()?;
            let line = line.split('#').next().unwrap_or_default();
            match line.trim_end().strip_suffix('\\') {
                Some(head) => {
                    out.push_str(head);
                    out.push(' ');
                }
                None => {
                    out.push_str(line);
                    return Some(out);
                }
            }
        }
    })
}

/// A face corner's vertex index (`v`, `v/vt`, `v//vn` or `v/vt/vn`; negative counts back).
fn corner(token: &str, count: usize) -> Option<u32> {
    index(token.split('/').next()?, count)
}

/// A face corner's texture coordinate index (`v/vt` or `v/vt/vn`).
fn corner_uv(token: &str, count: usize) -> Option<u32> {
    index(token.split('/').nth(1)?, count)
}

/// An index counted from 1 (negative: back from the end), checked against `count`.
fn index(token: &str, count: usize) -> Option<u32> {
    let v: i64 = token.parse().ok()?;
    let index = match v {
        0 => return None,
        v if v > 0 => v - 1,
        v => count as i64 + v,
    };
    u32::try_from(index).ok().filter(|i| (*i as usize) < count)
}

fn parse(text: &str, frame: Frame) -> Result<Parsed, ObjError> {
    let groups_split = !statements(text).any(|s| s.split_whitespace().next() == Some("o"));
    let mut out =
        Parsed { vertices: Vec::new(), uvs: Vec::new(), objects: Vec::new(), libraries: Vec::new(), used: Vec::new() };
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut material: Option<usize> = None;
    let mut faces = 0usize;
    let mut current = Object { name: String::new(), faces: Vec::new() };
    for statement in statements(text) {
        let statement = statement.trim();
        let (keyword, rest) = statement.split_once(char::is_whitespace).unwrap_or((statement, ""));
        let rest = rest.trim();
        match keyword {
            "v" => {
                if out.vertices.len() >= MAX_VERTICES {
                    return Err(ObjError::TooLarge);
                }
                let mut values = rest.split_whitespace().map(|t| t.parse::<f64>().ok());
                let point = match (values.next(), values.next(), values.next()) {
                    (Some(Some(x)), Some(Some(y)), Some(Some(z))) => {
                        let p = if frame.y_up { DVec3::new(x, -z, y) } else { DVec3::new(x, y, z) };
                        Some(p * frame.scale).filter(|p| p.is_finite())
                    }
                    _ => None,
                };
                out.vertices.push(point);
            }
            "vt" => {
                if out.uvs.len() >= MAX_VERTICES {
                    return Err(ObjError::TooLarge);
                }
                let mut values = rest.split_whitespace().map(|t| t.parse::<f64>().ok());
                let uv = match (values.next(), values.next()) {
                    (Some(Some(u)), Some(Some(v))) => Some(DVec2::new(u, v)),
                    (Some(Some(u)), None) => Some(DVec2::new(u, 0.0)),
                    _ => None,
                };
                out.uvs.push(uv.filter(|p| p.is_finite()));
            }
            "f" => {
                let count = out.vertices.len();
                let face: Option<Vec<u32>> =
                    rest.split_whitespace().take(MAX_CORNERS).map(|t| corner(t, count)).collect();
                let Some(face) = face.filter(|f| f.len() >= 3) else { continue };
                let uv_count = out.uvs.len();
                let uvs: Vec<u32> = rest
                    .split_whitespace()
                    .take(MAX_CORNERS)
                    .map(|t| corner_uv(t, uv_count))
                    .collect::<Option<_>>()
                    .unwrap_or_default();
                faces += 1;
                if faces > crate::skp::MAX_FACES {
                    return Err(ObjError::TooLarge);
                }
                current.faces.push((material, face, uvs));
            }
            "o" | "g" if keyword == "o" || groups_split => {
                let name = rest.to_string();
                let done = std::mem::replace(&mut current, Object { name, faces: Vec::new() });
                if !done.faces.is_empty() {
                    if out.objects.len() >= MAX_OBJECTS {
                        return Err(ObjError::TooLarge);
                    }
                    out.objects.push(done);
                }
            }
            "usemtl" => {
                material = (!rest.is_empty()).then(|| {
                    let next = out.used.len();
                    *used.entry(rest.to_string()).or_insert_with(|| {
                        out.used.push(rest.to_string());
                        next
                    })
                });
            }
            "mtllib" if !rest.is_empty() && out.libraries.len() < 16 => out.libraries.push(rest.to_string()),
            _ => {}
        }
    }
    if !current.faces.is_empty() {
        if out.objects.len() >= MAX_OBJECTS {
            return Err(ObjError::TooLarge);
        }
        out.objects.push(current);
    }
    Ok(out)
}

/// A material of a library: diffuse color (`Kd`, kept linear 0 to 1), opacity
/// (`d`, or `Tr` = 1 - d) and diffuse picture (`map_Kd`, a file name).
#[derive(Debug, Clone, PartialEq)]
pub struct Mtl {
    pub color: [f64; 3],
    pub opacity: f64,
    pub map: Option<String>,
}

impl Default for Mtl {
    fn default() -> Self {
        Self { color: [to_linear(200); 3], opacity: 1.0, map: None }
    }
}

/// Options of `map_Kd` and how many values each takes at most.
const MAP_OPTIONS: [(&str, usize); 11] = [
    ("-blendu", 1),
    ("-blendv", 1),
    ("-boost", 1),
    ("-cc", 1),
    ("-clamp", 1),
    ("-imfchan", 1),
    ("-mm", 2),
    ("-o", 3),
    ("-s", 3),
    ("-t", 3),
    ("-texres", 1),
];

/// The file name of a `map_Kd` statement, past its options (it may hold spaces).
fn map_file(rest: &str) -> Option<String> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    while let Some(t) = tokens.get(i) {
        let Some((_, n)) = MAP_OPTIONS.iter().find(|(o, _)| o.eq_ignore_ascii_case(t)) else { break };
        // An option takes up to `n` numbers (or on/off, or a channel letter).
        i += 1;
        for _ in 0..*n {
            match tokens.get(i) {
                Some(v)
                    if v.parse::<f64>().is_ok() || matches!(*v, "on" | "off" | "r" | "g" | "b" | "m" | "l" | "z") =>
                {
                    i += 1
                }
                _ => break,
            }
        }
    }
    let name = tokens.get(i..)?.join(" ");
    (!name.is_empty()).then_some(name)
}

/// The materials of a material library.
pub fn read_mtl(text: &str) -> HashMap<String, Mtl> {
    let mut out: HashMap<String, Mtl> = HashMap::new();
    let mut name: Option<String> = None;
    for statement in statements(text) {
        let statement = statement.trim();
        let (keyword, rest) = statement.split_once(char::is_whitespace).unwrap_or((statement, ""));
        let rest = rest.trim();
        if keyword == "newmtl" {
            out.entry(rest.to_string()).or_default();
            name = Some(rest.to_string());
            continue;
        }
        let Some(m) = name.as_ref().and_then(|n| out.get_mut(n)) else { continue };
        let numbers: Vec<f64> = rest.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        match keyword {
            "Kd" => {
                if let [r, g, b, ..] = numbers[..]
                    && [r, g, b].iter().all(|v| v.is_finite())
                {
                    // Read as display values (sRGB), as viewers do.
                    m.color = [r, g, b].map(|v| to_linear((v.clamp(0.0, 1.0) * 255.0).round() as u8));
                }
            }
            "d" => {
                if let Some(d) = numbers.first().filter(|d| d.is_finite()) {
                    m.opacity = d.clamp(0.0, 1.0);
                }
            }
            "Tr" => {
                if let Some(t) = numbers.first().filter(|t| t.is_finite()) {
                    m.opacity = 1.0 - t.clamp(0.0, 1.0);
                }
            }
            "map_Kd" => m.map = map_file(rest),
            _ => {}
        }
    }
    out
}

/// A file a model names, looked for by file name only in `dir` (a file
/// must not reach elsewhere on the disk), at most `limit` bytes.
fn beside(dir: &Path, name: &str, limit: u64) -> Option<Vec<u8>> {
    let path = dir.join(Path::new(name.trim()).file_name()?);
    let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= limit);
    small.then(|| std::fs::read(&path).ok()).flatten()
}

/// The material libraries a file names, looked for in `dir`.
fn libraries(dir: Option<&Path>, names: &[String]) -> HashMap<String, Mtl> {
    let mut out = HashMap::new();
    let Some(dir) = dir else { return out };
    // A name may hold spaces; else the statement lists several files.
    let files: std::collections::BTreeSet<&str> =
        names.iter().flat_map(|n| std::iter::once(n.as_str()).chain(n.split_whitespace())).collect();
    for file in files {
        let Some(bytes) = beside(dir, file, MAX_MTL_BYTES) else { continue };
        for (k, v) in read_mtl(&String::from_utf8_lossy(&bytes)) {
            out.entry(k).or_insert(v);
        }
    }
    out
}

/// The texture repeats of the materials `used`, from the faces' coordinates.
fn uv_scales(parsed: &Parsed) -> Vec<UvScale> {
    let mut scales = vec![UvScale::default(); parsed.used.len()];
    let p = |i: u32| parsed.vertices.get(i as usize).copied().flatten();
    let t = |i: u32| parsed.uvs.get(i as usize).copied().flatten();
    for (material, face, uvs) in parsed.objects.iter().flat_map(|o| &o.faces) {
        let Some(scale) = material.and_then(|m| scales.get_mut(m)) else { continue };
        if uvs.len() != face.len() {
            continue;
        }
        for k in 1..face.len().saturating_sub(1) {
            let (Some(a), Some(b), Some(c)) = (p(face[0]), p(face[k]), p(face[k + 1])) else { continue };
            let (Some(ta), Some(tb), Some(tc)) = (t(uvs[0]), t(uvs[k]), t(uvs[k + 1])) else { continue };
            scale.add([a, b, c], [ta, tb, tc]);
        }
    }
    scales
}

/// Convert the OBJ `text` (its material libraries looked for in `dir`) to
/// types and placements; `merge` makes the whole file one type at the origin
/// of the file, named `name`.
pub fn convert(text: &str, dir: Option<&Path>, frame: Frame, name: &str, merge: bool) -> Result<Converted, ObjError> {
    let mut parsed = parse(text, frame)?;
    let libraries = libraries(dir, &parsed.libraries);
    let mut out = Converted::default();
    for (name, scale) in parsed.used.iter().zip(uv_scales(&parsed)) {
        let mtl = libraries.get(name).cloned().unwrap_or_default();
        let limit = cc_core::material::MAX_TEXTURE_BYTES as u64;
        let image = dir
            .zip(mtl.map.as_deref())
            .and_then(|(dir, map)| beside(dir, map, limit))
            .and_then(crate::import_material::texture_image);
        let material = crate::import_material::textured(name.clone(), mtl.color, mtl.opacity, image, scale.repeat());
        out.materials.push(material);
    }
    if merge && parsed.objects.len() > 1 {
        let faces = parsed.objects.drain(..).flat_map(|o| o.faces).collect();
        parsed.objects.push(Object { name: String::new(), faces });
    }
    let single = parsed.objects.len() == 1;
    for (i, object) in parsed.objects.iter().enumerate() {
        let points = |face: &[u32]| -> Option<Vec<DVec3>> {
            face.iter().map(|&i| parsed.vertices.get(i as usize).copied().flatten()).collect()
        };
        // Insertion point: the center of the footprint, at the bottom (the file's origin when merged).
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for (_, face, _) in &object.faces {
            for p in points(face).unwrap_or_default() {
                (lo, hi) = (lo.min(p), hi.max(p));
            }
        }
        if !lo.is_finite() || !hi.is_finite() {
            continue;
        }
        let origin = if merge { DVec3::ZERO } else { DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z) };
        let mut pieces: BTreeMap<Option<usize>, MeshBuild> = BTreeMap::new();
        for (material, face, _) in &object.faces {
            let Some(face) = points(face) else { continue };
            let face: Vec<DVec3> = face.iter().map(|p| *p - origin).collect();
            let mesh = pieces.entry(*material).or_default();
            for polygon in planar_pieces(&face) {
                out.faces += 1;
                mesh.add(&polygon);
            }
        }
        let type_name = match object.name.trim() {
            n if !n.is_empty() => n.to_string(),
            _ if single || merge => name.to_string(),
            _ => format!("{name} {}", i + 1),
        };
        let Some(def) = mesh_definition(&type_name, pieces, &out.materials) else { continue };
        out.types.push(SkpType { name: type_name, def });
        out.placements.push(SkpPlacement {
            ty: out.types.len() - 1,
            position: DVec2::new(origin.x, origin.y),
            z: origin.z,
            rotation_deg: 0.0,
        });
    }
    Ok(out)
}

/// `face` as it is when flat, else as a fan of triangles; nothing when it has no area.
fn planar_pieces(face: &[DVec3]) -> Vec<Vec<DVec3>> {
    // Newell's normal.
    let mut n = DVec3::ZERO;
    for (i, a) in face.iter().enumerate() {
        let b = face[(i + 1) % face.len()];
        n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    let Some(normal) = n.try_normalize() else { return Vec::new() };
    let base = face[0];
    if face.len() == 3 || face.iter().all(|p| (*p - base).dot(normal).abs() <= PLANE_TOLERANCE) {
        return vec![face.to_vec()];
    }
    face.windows(2)
        .skip(1)
        .map(|w| vec![base, w[0], w[1]])
        .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 0.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_count_from_one_and_back() {
        assert_eq!(corner("1", 3), Some(0));
        assert_eq!(corner("3/7/2", 3), Some(2));
        assert_eq!(corner("-1//4", 3), Some(2));
        assert_eq!(corner("4", 3), None);
        assert_eq!(corner("0", 3), None);
        assert_eq!(corner("-4", 3), None);
        assert_eq!(corner("x", 3), None);
    }

    #[test]
    fn bent_faces_become_triangles() {
        let flat = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
        assert_eq!(planar_pieces(&flat).len(), 1);
        let bent = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.5), DVec3::Y];
        assert_eq!(planar_pieces(&bent).len(), 2);
        assert!(planar_pieces(&[DVec3::ZERO, DVec3::X, DVec3::X * 2.0]).is_empty());
    }

    #[test]
    fn materials_read_color_opacity_and_picture() {
        let text =
            "newmtl Red\nKd 1 0 0\nd 0.5\nmap_Kd -s 2 2 1 -clamp on my brick.png\nnewmtl Plain\nKd nan 1\nTr 0.25\n";
        let m = read_mtl(text);
        let red = &m["Red"];
        assert_eq!((red.color, red.opacity, red.map.as_deref()), ([1.0, 0.0, 0.0], 0.5, Some("my brick.png")));
        let plain = &m["Plain"];
        assert_eq!((plain.color, plain.opacity, plain.map.as_deref()), (Mtl::default().color, 0.75, None));
        assert_eq!(map_file("-o 1 2 3"), None);
        assert_eq!(map_file("tex.jpg"), Some("tex.jpg".into()));
    }
}
