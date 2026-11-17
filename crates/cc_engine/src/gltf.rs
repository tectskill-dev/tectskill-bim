//! glTF 2.0 models (`.glb`, or `.gltf` with its buffers embedded as data
//! URIs or in files next to it) as component types: each node of the scene's
//! root, with everything below it, becomes a type of fixed mesh pieces, one
//! per material, placed by an element at the center of its footprint and the
//! bottom of its extent. glTF lengths are meters and its Y axis points up,
//! so nothing needs asking. Materials take their base color and its
//! texture (a picture in a buffer, a data URI or a file next to the model),
//! repeating as the texture coordinates say (see [`crate::import_material`]),
//! and their opacity when blended.
//!
//! Files are hostile input: every index, offset and count is checked, the
//! node graph is walked with a depth limit and a visited set, and the sizes
//! read are bounded. Compressed meshes (Draco, meshopt) are not read.

use crate::import_material::{ImportedMaterial, UvScale};
use crate::skp::{Converted, MeshBuild, SkpPlacement, SkpType, mesh_definition};
use glam::{DMat4, DQuat, DVec2, DVec3};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Largest file read, in bytes (also each external buffer).
pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Deepest node nesting followed.
const MAX_DEPTH: usize = 64;
/// Node visits of one import (instanced nodes count each time).
const MAX_VISITS: usize = 100_000;
/// Elements an accessor may have.
const MAX_COUNT: usize = 50_000_000;
/// Root objects (types) an import makes.
const MAX_OBJECTS: usize = 20_000;

const GLB_MAGIC: &[u8; 4] = b"glTF";
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

#[derive(Debug, Clone, PartialEq)]
pub enum GltfError {
    /// Not a glTF 2.0 file, or its structure is broken.
    Invalid,
    /// The meshes need an extension this reader lacks (its name).
    Unsupported(String),
    /// A buffer the meshes read could not be found or read (its URI).
    MissingBuffer(String),
    /// More faces or objects than an import takes.
    TooLarge,
}

/// The JSON part and the binary chunk of a file (`.glb`, or plain `.gltf` text).
fn split(bytes: &[u8]) -> Result<(Value, Option<&[u8]>), GltfError> {
    if bytes.get(..4) != Some(GLB_MAGIC) {
        let json = serde_json::from_slice(bytes).map_err(|_| GltfError::Invalid)?;
        return Ok((json, None));
    }
    let word = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
    if word(4) != Some(2) {
        return Err(GltfError::Invalid);
    }
    let total = (word(8).ok_or(GltfError::Invalid)? as usize).min(bytes.len());
    let (mut json, mut bin) = (None, None);
    let mut at = 12;
    while at + 8 <= total {
        let (Some(len), Some(kind)) = (word(at), word(at + 4)) else { break };
        let start = at + 8;
        let Some(chunk) = start.checked_add(len as usize).and_then(|end| bytes.get(start..end.min(total))) else {
            break;
        };
        match kind {
            CHUNK_JSON if json.is_none() => json = Some(chunk),
            CHUNK_BIN if bin.is_none() => bin = Some(chunk),
            _ => {}
        }
        // Chunks are padded to four bytes.
        at = start + (len as usize).div_ceil(4) * 4;
    }
    let json = serde_json::from_slice(json.ok_or(GltfError::Invalid)?).map_err(|_| GltfError::Invalid)?;
    Ok((json, bin))
}

/// A buffer's bytes: the GLB binary chunk (buffer 0 without a URI), a base64
/// data URI, or a file looked for by name only in `dir` (a file must not
/// reach elsewhere on the disk).
fn load_buffer(buffer: &Value, index: usize, bin: Option<&[u8]>, dir: Option<&Path>) -> Result<Vec<u8>, GltfError> {
    let Some(uri) = buffer.get("uri").and_then(Value::as_str) else {
        return match (index, bin) {
            (0, Some(bin)) => Ok(bin.to_vec()),
            _ => Err(GltfError::MissingBuffer(format!("#{index}"))),
        };
    };
    let missing = || GltfError::MissingBuffer(uri.chars().take(80).collect());
    if let Some(data) = uri.strip_prefix("data:") {
        let (_, encoded) = data.split_once(";base64,").ok_or_else(missing)?;
        return cc_core::raster::base64_decode(encoded).ok_or_else(missing);
    }
    let decoded = percent_decode(uri);
    let file = Path::new(&decoded).file_name().ok_or_else(missing)?;
    let path = dir.ok_or_else(missing)?.join(file);
    let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES);
    small.then(|| std::fs::read(&path).ok()).flatten().ok_or_else(missing)
}

/// `%20` and the like in a URI.
fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The file's parts, with its buffers loaded.
struct Doc {
    json: Value,
    buffers: Vec<Option<Vec<u8>>>,
    /// Where files the model names are looked for.
    dir: Option<std::path::PathBuf>,
}

impl Doc {
    /// The bytes of image `index`: a buffer view, a data URI or a file next
    /// to the model (by name only).
    fn image(&self, index: Option<&Value>) -> Option<Vec<u8>> {
        let image = self.item("images", index)?;
        let limit = cc_core::material::MAX_TEXTURE_BYTES;
        if let Some(view) = self.item("bufferViews", image.get("bufferView")) {
            let buffer = self.buffers.get(usize::try_from(view.get("buffer")?.as_u64()?).ok()?)?.as_ref()?;
            let start = usize::try_from(view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0)).ok()?;
            let len = usize::try_from(view.get("byteLength")?.as_u64()?).ok().filter(|l| *l <= limit)?;
            return buffer.get(start..start.checked_add(len)?).map(<[u8]>::to_vec);
        }
        let uri = image.get("uri")?.as_str()?;
        if let Some(data) = uri.strip_prefix("data:") {
            let (_, encoded) = data.split_once(";base64,")?;
            return cc_core::raster::base64_decode(encoded).filter(|b| b.len() <= limit);
        }
        let decoded = percent_decode(uri);
        let path = self.dir.as_ref()?.join(Path::new(&decoded).file_name()?);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= limit as u64);
        small.then(|| std::fs::read(&path).ok()).flatten()
    }

    /// The base color texture of a material: its image (the texture's
    /// source, or a WebP source) and the texture coordinate set it reads.
    fn base_texture(&self, material: &Value) -> Option<(Option<&Value>, u64)> {
        let info = material.get("pbrMetallicRoughness")?.get("baseColorTexture")?;
        let texture = self.item("textures", info.get("index"))?;
        let webp = texture.get("extensions").and_then(|e| e.get("EXT_texture_webp")).and_then(|w| w.get("source"));
        let set = info.get("texCoord").and_then(Value::as_u64).unwrap_or(0);
        Some((texture.get("source").or(webp), set))
    }

    fn array(&self, key: &str) -> &[Value] {
        self.json.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }

    fn item(&self, key: &str, index: Option<&Value>) -> Option<&Value> {
        self.array(key).get(usize::try_from(index?.as_u64()?).ok()?)
    }

    /// The values of an accessor as numbers, `width` per element (normalized
    /// integers mapped to 0..1 or -1..1); `None` when anything is out of range.
    fn accessor(&self, index: Option<&Value>, width: usize) -> Option<Vec<f64>> {
        let acc = self.item("accessors", index)?;
        let count = usize::try_from(acc.get("count")?.as_u64()?).ok().filter(|c| *c <= MAX_COUNT)?;
        let components = match acc.get("type")?.as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        if components != width {
            return None;
        }
        let kind = acc.get("componentType")?.as_u64()?;
        let size = match kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return None,
        };
        let normalized = acc.get("normalized").and_then(Value::as_bool).unwrap_or(false);
        let view = self.item("bufferViews", acc.get("bufferView"))?;
        let buffer = self.buffers.get(usize::try_from(view.get("buffer")?.as_u64()?).ok()?)?.as_ref()?;
        let offset = |v: &Value| v.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
        let start = usize::try_from(offset(view).checked_add(offset(acc))?).ok()?;
        let view_end = usize::try_from(offset(view).checked_add(view.get("byteLength")?.as_u64()?)?).ok()?;
        let element = size * components;
        let stride = match view.get("byteStride").and_then(Value::as_u64) {
            Some(s) => usize::try_from(s).ok().filter(|s| *s >= element)?,
            None => element,
        };
        if count > 0 {
            let end = start.checked_add((count - 1).checked_mul(stride)?)?.checked_add(element)?;
            if end > view_end || end > buffer.len() {
                return None;
            }
        }
        let mut out = Vec::with_capacity(count * components);
        for i in 0..count {
            for c in 0..components {
                let at = start + i * stride + c * size;
                let b = buffer.get(at..at + size)?;
                let v = match kind {
                    5120 => f64::from(b[0] as i8),
                    5121 => f64::from(b[0]),
                    5122 => f64::from(i16::from_le_bytes([b[0], b[1]])),
                    5123 => f64::from(u16::from_le_bytes([b[0], b[1]])),
                    5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                    _ => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                };
                let v = match (normalized, kind) {
                    (true, 5120) => (v / 127.0).max(-1.0),
                    (true, 5121) => v / 255.0,
                    (true, 5122) => (v / 32767.0).max(-1.0),
                    (true, 5123) => v / 65535.0,
                    _ => v,
                };
                out.push(v);
            }
        }
        Some(out)
    }
}

/// A node's own transform: its `matrix` (column-major), else translation,
/// rotation and scale.
fn local(node: &Value) -> DMat4 {
    let numbers = |key: &str, n: usize| -> Option<Vec<f64>> {
        let values: Vec<f64> = node.get(key)?.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
        (values.len() == n && values.iter().all(|v| v.is_finite())).then_some(values)
    };
    if let Some(m) = numbers("matrix", 16) {
        return DMat4::from_cols_slice(&m);
    }
    let t = numbers("translation", 3).map_or(DVec3::ZERO, |v| DVec3::new(v[0], v[1], v[2]));
    let r = numbers("rotation", 4)
        .map(|v| DQuat::from_xyzw(v[0], v[1], v[2], v[3]))
        .filter(|q| q.length() > 1e-12)
        .map(DQuat::normalize)
        .unwrap_or(DQuat::IDENTITY);
    let s = numbers("scale", 3).map_or(DVec3::ONE, |v| DVec3::new(v[0], v[1], v[2]));
    DMat4::from_scale_rotation_translation(s, r, t)
}

/// One root object: its triangles by material (index into the file's).
#[derive(Default)]
struct Object {
    name: String,
    triangles: Vec<(Option<usize>, [DVec3; 3])>,
}

struct Walker<'a> {
    doc: &'a Doc,
    visits: usize,
    faces: usize,
    /// Texture repeats of the file's materials.
    scales: Vec<UvScale>,
}

impl Walker<'_> {
    /// Add the triangles of `node` and everything below it, placed by `parent`.
    fn node(
        &mut self,
        index: usize,
        parent: DMat4,
        depth: usize,
        path: &mut HashSet<usize>,
        out: &mut Object,
    ) -> Result<(), GltfError> {
        self.visits += 1;
        if depth > MAX_DEPTH || self.visits > MAX_VISITS || !path.insert(index) {
            return Ok(());
        }
        let Some(node) = self.doc.array("nodes").get(index) else {
            path.remove(&index);
            return Ok(());
        };
        let world = parent * local(node);
        if let Some(mesh) = self.doc.item("meshes", node.get("mesh")) {
            if out.name.is_empty() {
                out.name = name_of(mesh);
            }
            self.mesh(mesh, world, out)?;
        }
        for child in node.get("children").and_then(Value::as_array).into_iter().flatten() {
            if let Some(child) = child.as_u64().and_then(|c| usize::try_from(c).ok()) {
                self.node(child, world, depth + 1, path, out)?;
            }
        }
        path.remove(&index);
        Ok(())
    }

    fn mesh(&mut self, mesh: &Value, world: DMat4, out: &mut Object) -> Result<(), GltfError> {
        // Mirrored placements turn the winding over.
        let flip = world.determinant() < 0.0;
        for primitive in mesh.get("primitives").and_then(Value::as_array).into_iter().flatten() {
            let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            if !matches!(mode, 4..=6) {
                continue;
            }
            let Some(positions) = self.doc.accessor(primitive.get("attributes").and_then(|a| a.get("POSITION")), 3)
            else {
                continue;
            };
            let points: Vec<DVec3> = positions
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| {
                    let w = world.transform_point3(DVec3::new(p[0], p[1], p[2]));
                    // Y up to Z up.
                    DVec3::new(w.x, -w.z, w.y)
                })
                .collect();
            let indices: Vec<usize> = match primitive.get("indices") {
                Some(i) => match self.doc.accessor(Some(i), 1) {
                    Some(v) => v.into_iter().map(|i| i as usize).collect(),
                    None => continue,
                },
                None => (0..points.len()).collect(),
            };
            let material = primitive.get("material").and_then(Value::as_u64).and_then(|m| usize::try_from(m).ok());
            // The texture coordinates the material's base color reads.
            let set = material.and_then(|m| self.doc.array("materials").get(m)).and_then(|m| self.doc.base_texture(m));
            let uvs: Vec<glam::DVec2> = set
                .and_then(|(_, set)| {
                    let attributes = primitive.get("attributes")?;
                    self.doc.accessor(attributes.get(format!("TEXCOORD_{set}")), 2)
                })
                .map(|v| v.as_chunks::<2>().0.iter().map(|t| glam::DVec2::new(t[0], t[1])).collect())
                .unwrap_or_default();
            let corners: Vec<[usize; 3]> = match mode {
                4 => indices.as_chunks::<3>().0.to_vec(),
                // Strips alternate their winding.
                5 => indices
                    .windows(3)
                    .enumerate()
                    .map(|(k, t)| if k % 2 == 0 { [t[0], t[1], t[2]] } else { [t[1], t[0], t[2]] })
                    .collect(),
                _ => indices.get(1..).unwrap_or_default().windows(2).map(|t| [indices[0], t[0], t[1]]).collect(),
            };
            for corner in corners {
                let [a, b, c] = corner;
                let (Some(a), Some(b), Some(c)) = (points.get(a), points.get(b), points.get(c)) else { continue };
                if !(a.is_finite() && b.is_finite() && c.is_finite())
                    || (*b - *a).cross(*c - *a).length_squared() == 0.0
                {
                    continue;
                }
                self.faces += 1;
                if self.faces > crate::skp::MAX_FACES {
                    return Err(GltfError::TooLarge);
                }
                if let (Some(scale), Some(ta), Some(tb), Some(tc)) = (
                    material.and_then(|m| self.scales.get_mut(m)),
                    uvs.get(corner[0]),
                    uvs.get(corner[1]),
                    uvs.get(corner[2]),
                ) {
                    scale.add([*a, *b, *c], [*ta, *tb, *tc]);
                }
                out.triangles.push((material, if flip { [*a, *c, *b] } else { [*a, *b, *c] }));
            }
        }
        Ok(())
    }
}

fn name_of(v: &Value) -> String {
    v.get("name").and_then(Value::as_str).map(|n| n.trim().to_string()).unwrap_or_default()
}

/// The file's materials: name (or `<name> material n`), base color times
/// its texture's average, the texture repeating as `scales` say, and the
/// opacity of blended ones.
fn materials(doc: &Doc, name: &str, scales: &[UvScale]) -> Vec<ImportedMaterial> {
    doc.array("materials")
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let own = name_of(m);
            let label = if own.is_empty() { format!("{name} material {}", i + 1) } else { own };
            let factor = m.get("pbrMetallicRoughness").and_then(|p| p.get("baseColorFactor")).and_then(Value::as_array);
            let factor: Vec<f64> = factor.map(|f| f.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let [r, g, b, a] = match factor[..] {
                [r, g, b, a] if [r, g, b, a].iter().all(|v| v.is_finite()) => [r, g, b, a].map(|v| v.clamp(0.0, 1.0)),
                _ => [1.0; 4],
            };
            let blended = m.get("alphaMode").and_then(Value::as_str) == Some("BLEND");
            let image = doc
                .base_texture(m)
                .and_then(|(source, _)| doc.image(source))
                .and_then(crate::import_material::texture_image);
            let repeat = scales.get(i).and_then(UvScale::repeat);
            crate::import_material::textured(label, [r, g, b], if blended { a } else { 1.0 }, image, repeat)
        })
        .collect()
}

/// Convert the glTF file `bytes` (external buffers looked for in `dir`) to
/// types and placements; `merge` makes the whole scene one type at the
/// file's origin, named `name`.
pub fn convert(bytes: &[u8], dir: Option<&Path>, name: &str, merge: bool) -> Result<Converted, GltfError> {
    let (json, bin) = split(bytes)?;
    let version = json.get("asset").and_then(|a| a.get("version")).and_then(Value::as_str).unwrap_or_default();
    if !version.starts_with('2') {
        return Err(GltfError::Invalid);
    }
    let required = json.get("extensionsRequired").and_then(Value::as_array).into_iter().flatten();
    if let Some(ext) = required
        .filter_map(Value::as_str)
        .find(|e| matches!(*e, "KHR_draco_mesh_compression" | "EXT_meshopt_compression" | "KHR_meshopt_compression"))
    {
        return Err(GltfError::Unsupported(ext.to_string()));
    }
    let mut missing = None;
    let buffers: Vec<Option<Vec<u8>>> = json
        .get("buffers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .enumerate()
        .map(|(i, b)| load_buffer(b, i, bin, dir).map_err(|e| missing.get_or_insert(e).clone()).ok())
        .collect();
    let doc = Doc { json, buffers, dir: dir.map(Path::to_path_buf) };
    // The scene's roots; without scenes, the nodes no other node holds.
    let scene = doc.item("scenes", doc.json.get("scene")).or_else(|| doc.array("scenes").first());
    let roots: Vec<usize> = match scene {
        Some(s) => s
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_u64().and_then(|n| usize::try_from(n).ok()))
            .collect(),
        None => {
            let held: HashSet<u64> = doc
                .array("nodes")
                .iter()
                .flat_map(|n| n.get("children").and_then(Value::as_array).into_iter().flatten())
                .filter_map(Value::as_u64)
                .collect();
            (0..doc.array("nodes").len()).filter(|i| !held.contains(&(*i as u64))).collect()
        }
    };
    let scales = vec![UvScale::default(); doc.array("materials").len()];
    let mut walker = Walker { doc: &doc, visits: 0, faces: 0, scales };
    let mut objects: Vec<Object> = Vec::new();
    for root in roots {
        let mut object =
            Object { name: doc.array("nodes").get(root).map(name_of).unwrap_or_default(), ..Default::default() };
        walker.node(root, DMat4::IDENTITY, 0, &mut HashSet::new(), &mut object)?;
        if object.triangles.is_empty() {
            continue;
        }
        if objects.len() >= MAX_OBJECTS {
            return Err(GltfError::TooLarge);
        }
        objects.push(object);
    }
    if objects.is_empty()
        && let Some(e) = missing
    {
        return Err(e);
    }
    if merge && objects.len() > 1 {
        let triangles = objects.drain(..).flat_map(|o| o.triangles).collect();
        objects.push(Object { name: String::new(), triangles });
    }
    let mut out = Converted { materials: materials(&doc, name, &walker.scales), ..Default::default() };
    let single = objects.len() == 1;
    for (i, object) in objects.iter().enumerate() {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for p in object.triangles.iter().flat_map(|(_, t)| t) {
            (lo, hi) = (lo.min(*p), hi.max(*p));
        }
        if !lo.is_finite() || !hi.is_finite() {
            continue;
        }
        // Insertion point: the center of the footprint, at the bottom (the file's origin when merged).
        let origin = if merge { DVec3::ZERO } else { DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z) };
        let mut pieces: BTreeMap<Option<usize>, MeshBuild> = BTreeMap::new();
        for (material, t) in &object.triangles {
            // A material the file does not list draws in the default one.
            let material = material.filter(|m| *m < out.materials.len());
            pieces.entry(material).or_default().add(&t.map(|p| p - origin));
            out.faces += 1;
        }
        let type_name = match object.name.as_str() {
            n if !n.is_empty() && !merge => n.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_are_percent_decoded() {
        assert_eq!(percent_decode("my%20model.bin"), "my model.bin");
        assert_eq!(percent_decode("bad%zz%2"), "bad%zz%2");
    }

    #[test]
    fn broken_glb_headers_are_invalid() {
        assert_eq!(split(b"glTF\x01\0\0\0\x0c\0\0\0").err(), Some(GltfError::Invalid));
        assert_eq!(split(b"glTF\x02\0\0\0\xff\xff\xff\xff\x10\0\0\0JSON").err(), Some(GltfError::Invalid));
        assert_eq!(split(b"not json").err(), Some(GltfError::Invalid));
    }
}
