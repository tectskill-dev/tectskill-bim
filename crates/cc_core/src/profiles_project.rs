//! Structural profiles in the project: lookups and repair of loaded files.

use crate::structural_profile::{MAX_NAME, MAX_PROFILES, ProfileDef, ProfileMaterial, ProfileShape, SectionOutline};
use crate::{ColumnSection, ElementTypeId, ProfileId, Project, SectionShape, TypeData, Uid};
use glam::DVec2;
use std::collections::BTreeSet;

/// The section a column or beam type draws: its profile's outline turned and
/// mirrored, or the legacy section's.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeSection {
    pub outline: SectionOutline,
    /// The profile it comes from; `None` for a legacy section.
    pub profile: Option<ProfileId>,
    pub material: ProfileMaterial,
    /// Whether the outline has arcs (drawn as short chords).
    pub curved: bool,
}

impl ProfileShape {
    /// The profile shape of a legacy column section (`width` along x,
    /// `depth` along y).
    pub fn from_column_section(s: ColumnSection) -> ProfileShape {
        match s {
            ColumnSection::Rectangular { width, depth } => ProfileShape::Rect { b: width, h: depth, chamfer: 0.0 },
            ColumnSection::Round { diameter } => ProfileShape::Round { d: diameter },
            ColumnSection::Pipe { diameter, wall } => ProfileShape::RoundTube { d: diameter, t: wall },
            ColumnSection::Profile { width, depth, shape } => ProfileShape::from_beam(width, depth, shape),
        }
    }

    /// The profile shape of a legacy `width` x `height` section filled by `shape`.
    pub fn from_beam(width: f64, height: f64, shape: SectionShape) -> ProfileShape {
        let (b, d) = (width, height);
        match shape {
            SectionShape::Solid => ProfileShape::Rect { b, h: d, chamfer: 0.0 },
            SectionShape::I { web, flange } => ProfileShape::I { d, bf: b, tw: web, tf: flange, r: 0.0 },
            SectionShape::Channel { web, flange } => ProfileShape::Channel { d, bf: b, tw: web, tf: flange, r: 0.0 },
            SectionShape::Angle { thickness } => ProfileShape::Angle { h: d, b, t: thickness, r: 0.0 },
            SectionShape::Tube { wall } => ProfileShape::RectTube { h: d, b, t: wall, ro: 0.0 },
        }
    }
}

/// A length for a profile name: millimeters, or centimeters when `cm`,
/// to a tenth at most (`200`, `9.5`).
fn name_len(v: f64, cm: bool) -> String {
    let v = if cm { v * 100.0 } else { v * 1000.0 };
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-9 { format!("{r:.0}") } else { format!("{r:.1}") }
}

/// The name a converted section's profile gets: `I 200x200x9x15`,
/// `Rect 20x40` (centimeters for concrete and timber rectangles and rounds,
/// as section tags show them), `Ø30`, `Tube 150x150x6`.
pub fn section_name(shape: &ProfileShape, material: ProfileMaterial) -> String {
    let cm = matches!(material, ProfileMaterial::Concrete | ProfileMaterial::Timber);
    let mm = |v: f64| name_len(v, false);
    match *shape {
        ProfileShape::I { d, bf, tw, tf, .. } => format!("I {}x{}x{}x{}", mm(d), mm(bf), mm(tw), mm(tf)),
        ProfileShape::Channel { d, bf, tw, tf, .. } => format!("U {}x{}x{}x{}", mm(d), mm(bf), mm(tw), mm(tf)),
        ProfileShape::Angle { h, b, t, .. } => format!("L {}x{}x{}", mm(h), mm(b), mm(t)),
        ProfileShape::RectTube { h, b, t, .. } => format!("Tube {}x{}x{}", mm(h), mm(b), mm(t)),
        ProfileShape::RoundTube { d, t } => format!("Ø{}x{}", mm(d), mm(t)),
        ProfileShape::Rect { b, h, .. } => format!("Rect {}x{}", name_len(b, cm), name_len(h, cm)),
        ProfileShape::Round { d } => format!("Ø{}", name_len(d, cm)),
        ProfileShape::Sketch(_) => "Sketch".to_string(),
        _ => {
            let values: Vec<String> = shape.params().into_iter().map(|(_, v)| mm(v)).collect();
            format!("{} {}", shape.code(), values.join("x"))
        }
    }
}

/// [`section_name`], or in an imperial project the same name with every
/// size in inches (`I 8"x8"x5/16"x7/16"`, `Rect 8"x8"`, `Ø12"`).
pub fn section_name_in(shape: &ProfileShape, material: ProfileMaterial, system: crate::MeasurementSystem) -> String {
    if system == crate::MeasurementSystem::Metric {
        return section_name(shape, material);
    }
    let i = crate::units::inch_text;
    match *shape {
        ProfileShape::I { d, bf, tw, tf, .. } => format!("I {}x{}x{}x{}", i(d), i(bf), i(tw), i(tf)),
        ProfileShape::Channel { d, bf, tw, tf, .. } => format!("U {}x{}x{}x{}", i(d), i(bf), i(tw), i(tf)),
        ProfileShape::Angle { h, b, t, .. } => format!("L {}x{}x{}", i(h), i(b), i(t)),
        ProfileShape::RectTube { h, b, t, .. } => format!("Tube {}x{}x{}", i(h), i(b), i(t)),
        ProfileShape::RoundTube { d, t } => format!("Ø{}x{}", i(d), i(t)),
        ProfileShape::Rect { b, h, .. } => format!("Rect {}x{}", i(b), i(h)),
        ProfileShape::Round { d } => format!("Ø{}", i(d)),
        ProfileShape::Sketch(_) => "Sketch".to_string(),
        _ => {
            let values: Vec<String> = shape.params().into_iter().map(|(_, v)| i(v)).collect();
            format!("{} {}", shape.code(), values.join("x"))
        }
    }
}

/// An annular sector section in its type's frame: the ring's `center`, the
/// unit `axis` from it through the middle of the sector, the radii and the
/// angle it turns (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectorPlacement {
    pub center: DVec2,
    pub axis: DVec2,
    pub r_in: f64,
    pub r_out: f64,
    pub sweep: f64,
}

/// `outline` mirrored across its vertical axis when `mirror`, then turned
/// `rotation_deg` counterclockwise (a non-finite angle reads as none).
fn place_outline(mut outline: SectionOutline, rotation_deg: f64, mirror: bool) -> SectionOutline {
    if mirror {
        for p in outline.outer.iter_mut().chain(outline.holes.iter_mut().flatten()) {
            p.x = -p.x;
        }
        outline.outer.reverse();
        outline.holes.iter_mut().for_each(|h| h.reverse());
    }
    if rotation_deg.is_finite() && rotation_deg != 0.0 {
        let r = DVec2::from_angle(rotation_deg.to_radians());
        for p in outline.outer.iter_mut().chain(outline.holes.iter_mut().flatten()) {
            *p = r.rotate(*p);
        }
    }
    outline
}

/// The parameters of a shape that round or cut its corners.
const RADII: [&str; 4] = ["r", "ri", "ro", "chamfer"];

impl ProfileShape {
    /// Whether two shapes are the same kind with every parameter within
    /// `tol` (meters; degrees for angles), so float noise from a file does
    /// not make a new profile. Sketches compare exactly.
    pub fn close_to(&self, other: &ProfileShape, tol: f64) -> bool {
        if matches!(self, Self::Sketch(_)) || matches!(other, Self::Sketch(_)) {
            return self == other;
        }
        if self.code() != other.code() {
            return false;
        }
        let (a, b) = (self.params(), other.params());
        a.len() == b.len() && a.iter().zip(&b).all(|((n, x), (m, y))| n == m && (x - y).abs() <= tol)
    }

    /// Whether this is a cold-formed shape (bent from sheet).
    pub fn is_cold_formed(&self) -> bool {
        matches!(self, Self::ColdU { .. } | Self::ColdLippedU { .. } | Self::ColdZ { .. } | Self::ColdHat { .. })
    }

    /// Whether the outline has arcs: a round, a radius above zero, or a
    /// sketch with arc or ellipse segments.
    pub fn has_arcs(&self) -> bool {
        match self {
            Self::Round { .. } | Self::RoundTube { .. } | Self::AnnularSector { .. } => true,
            Self::Sketch(s) => s
                .outline
                .iter()
                .chain(s.openings.iter().flatten())
                .any(|p| !p.radius.trim().is_empty() || p.ellipse.is_some()),
            _ => self.params().iter().any(|(n, v)| n != "chamfer" && RADII.contains(&n.as_str()) && *v > 0.0),
        }
    }
}

/// `shape` with every corner radius and chamfer at zero (sketches unchanged).
fn without_radii(shape: &ProfileShape) -> ProfileShape {
    let mut square = shape.clone();
    if !matches!(square, ProfileShape::Sketch(_)) {
        for name in RADII {
            square.set_param(name, 0.0);
        }
    }
    square
}

/// The pieces of a column or beam type that make its section.
struct SectionData<'a> {
    legacy: ProfileShape,
    material: &'a str,
    profile: Option<ProfileId>,
    rotation_deg: f64,
    mirror: bool,
}

fn section_data(data: &TypeData) -> Option<SectionData<'_>> {
    match data {
        TypeData::Column { section, material, profile, profile_rotation_deg, profile_mirror } => Some(SectionData {
            legacy: ProfileShape::from_column_section(*section),
            material,
            profile: *profile,
            rotation_deg: *profile_rotation_deg,
            mirror: *profile_mirror,
        }),
        TypeData::Beam { width, height, material, shape, profile, profile_rotation_deg, profile_mirror } => {
            Some(SectionData {
                legacy: ProfileShape::from_beam(*width, *height, *shape),
                material,
                profile: *profile,
                rotation_deg: *profile_rotation_deg,
                mirror: *profile_mirror,
            })
        }
        _ => None,
    }
}

/// The profile a column or beam type points to.
pub(crate) fn type_profile(data: &TypeData) -> Option<ProfileId> {
    section_data(data).and_then(|s| s.profile)
}

/// Whether two column or beam types have the same legacy section fields
/// (`section`, or `width`, `height` and `shape`).
pub(crate) fn same_legacy(a: &TypeData, b: &TypeData) -> bool {
    match (a, b) {
        (TypeData::Column { section: x, .. }, TypeData::Column { section: y, .. }) => x == y,
        (
            TypeData::Beam { width: w1, height: h1, shape: s1, .. },
            TypeData::Beam { width: w2, height: h2, shape: s2, .. },
        ) => w1 == w2 && h1 == h2 && s1 == s2,
        _ => false,
    }
}

/// The legacy box of a column or beam type, `(width, depth or height)`.
pub(crate) fn legacy_size(data: &TypeData) -> Option<(f64, f64)> {
    match data {
        TypeData::Column { section, .. } => Some(section.size()),
        TypeData::Beam { width, height, .. } => Some((*width, *height)),
        _ => None,
    }
}

/// `base`, or `base 2`, `base 3`… the first not in `taken` (compared
/// ignoring case), the base cut so the result fits `MAX_NAME`.
fn free_name(base: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let base: String = base.trim().chars().take(MAX_NAME - 12).collect();
    if !taken(&base) {
        return base;
    }
    let mut n = 2u64;
    loop {
        let candidate = format!("{base} {n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

impl ProfileDef {
    /// Fix what a hostile file can break without dropping the profile: the
    /// id (it must be the map key), the name and a missing uid. Returns the
    /// number of fixes.
    pub fn repair(&mut self, id: ProfileId) -> usize {
        let mut repairs = 0;
        if self.id != id {
            self.id = id;
            repairs += 1;
        }
        let trimmed: String = self.name.trim().chars().take(MAX_NAME).collect();
        let name = if trimmed.trim().is_empty() { format!("Profile {}", id.0) } else { trimmed };
        if name != self.name {
            self.name = name;
            repairs += 1;
        }
        if self.uid.is_nil() {
            self.uid = Uid::generate();
            repairs += 1;
        }
        repairs
    }
}

impl Project {
    /// A profile by id.
    pub fn profile(&self, id: ProfileId) -> Option<&ProfileDef> {
        self.profiles.get(&id)
    }

    /// A profile by name, ignoring case and surrounding spaces.
    pub fn profile_named(&self, name: &str) -> Option<&ProfileDef> {
        let wanted = name.trim().to_lowercase();
        self.profiles.values().find(|p| p.name.trim().to_lowercase() == wanted)
    }

    /// The profile a column or beam type points to, when it exists.
    pub fn type_profile_def(&self, data: &TypeData) -> Option<&ProfileDef> {
        type_profile(data).and_then(|id| self.profile(id))
    }

    /// The ring of a column or beam type whose section is an annular
    /// sector, as the type places it (turned and mirrored).
    pub fn type_sector(&self, data: &TypeData) -> Option<SectorPlacement> {
        let s = section_data(data)?;
        let shape = &s.profile.and_then(|id| self.profile(id))?.shape;
        let ProfileShape::AnnularSector { r_in, r_out, sweep } = *shape else { return None };
        let center = shape.sector_center()?;
        let turn = if s.rotation_deg.is_finite() { s.rotation_deg.to_radians() } else { 0.0 };
        let r = DVec2::from_angle(turn);
        // The sector is symmetric about its own Y axis: a mirror leaves it as it is.
        Some(SectorPlacement {
            center: r.rotate(center),
            axis: r.rotate(DVec2::Y),
            r_in,
            r_out,
            sweep: sweep.to_radians(),
        })
    }

    /// Whether a column or beam type's section is round (a solid or hollow
    /// circle): its profile's shape, else its legacy section. The legacy
    /// fields of a profile-made type are only its box, so readers ask this.
    pub fn section_is_round(&self, data: &TypeData) -> bool {
        let Some(s) = section_data(data) else { return false };
        let shape = s.profile.and_then(|id| self.profile(id)).map_or(&s.legacy, |d| &d.shape);
        matches!(shape, ProfileShape::Round { .. } | ProfileShape::RoundTube { .. })
    }

    /// The exact area of a column or beam type's section (its profile's,
    /// else its legacy section's): arcs count whole, where the outline only
    /// follows them with chords. `None` when the shape makes no outline.
    pub fn exact_section_area(&self, data: &TypeData) -> Option<f64> {
        let s = section_data(data)?;
        let shape = s.profile.and_then(|id| self.profile(id)).map_or(&s.legacy, |d| &d.shape);
        shape.area().ok()
    }

    /// The properties of a column or beam type's section for a material of
    /// `density` kg/m³. The area and the mass per meter are exact for arcs
    /// ([`Self::exact_section_area`]); the inertias are those of the outline,
    /// which follows arcs with chords.
    pub fn type_section_properties(&self, data: &TypeData, density: f64) -> Option<crate::SectionProperties> {
        let mut props = self.type_section(data)?.outline.properties(density);
        if let Some(exact) = self.exact_section_area(data).filter(|a| a.is_finite() && *a > 0.0)
            && props.area > 0.0
        {
            props.linear_mass *= exact / props.area;
            props.area = exact;
        }
        Some(props)
    }

    /// The column and beam types that use a profile.
    pub fn profile_users(&self, id: ProfileId) -> Vec<ElementTypeId> {
        self.types
            .values()
            .filter(|t| section_data(&t.data).is_some_and(|s| s.profile == Some(id)))
            .map(|t| t.id)
            .collect()
    }

    /// The profile material of a type's material: steel for metals, timber
    /// for wood, concrete otherwise.
    pub fn profile_material_for(&self, material: &str) -> ProfileMaterial {
        match self.material(material).map(|m| m.identity.class) {
            Some(crate::MaterialClass::Metal) => ProfileMaterial::Steel,
            Some(crate::MaterialClass::Wood) => ProfileMaterial::Timber,
            _ => ProfileMaterial::Concrete,
        }
    }

    /// The section a column or beam type draws: its profile's outline turned
    /// and mirrored, or the legacy section's (a legacy section whose shape
    /// does not fit its box reads as the box). `None` for other types or a
    /// profile whose outline fails.
    pub fn type_section(&self, data: &TypeData) -> Option<TypeSection> {
        self.section_of(data, false)
    }

    /// [`Self::type_section`] with every radius of the shape (`r`, `ri`,
    /// `ro`, `chamfer`) at zero: arcs replaced by their corners, as medium
    /// detail draws it. Falls back to the rounded outline when the shape
    /// fails without its radii; sketch profiles are unchanged.
    pub fn type_section_without_radii(&self, data: &TypeData) -> Option<TypeSection> {
        self.section_of(data, true)
    }

    fn section_of(&self, data: &TypeData, square: bool) -> Option<TypeSection> {
        let s = section_data(data)?;
        // The outline and whether it has arcs.
        let outline_of = |shape: &ProfileShape| {
            let flat = square.then(|| without_radii(shape)).and_then(|f| Some((f.outline().ok()?, f.has_arcs())));
            match flat {
                Some(found) => Ok(found),
                None => shape.outline().map(|o| (o, shape.has_arcs())),
            }
        };
        let ((outline, curved), profile, material) = match s.profile.and_then(|id| self.profile(id)) {
            Some(def) => (outline_of(&def.shape).ok()?, Some(def.id), def.material),
            None => {
                let outline = outline_of(&s.legacy).ok().or_else(|| {
                    let (b, h) = legacy_size(data)?;
                    Some((ProfileShape::Rect { b, h, chamfer: 0.0 }.outline().ok()?, false))
                })?;
                (outline, None, self.profile_material_for(s.material))
            }
        };
        Some(TypeSection { outline: place_outline(outline, s.rotation_deg, s.mirror), profile, material, curved })
    }

    /// `data` with its legacy section fields rewritten as the bounding box of
    /// its profile's placed outline (`Rectangular`, or `Solid` `width` x
    /// `height`), when the type points to a valid profile and its fields
    /// differ from that box; `None` when nothing needs to change (no
    /// profile, the legacy section is exactly the profile, or the outline
    /// fails).
    pub(crate) fn profile_box(&self, data: &TypeData) -> Option<TypeData> {
        const TOL: f64 = 1e-9;
        section_data(data)?.profile.and_then(|id| self.profile(id))?;
        if self.legacy_is_profile(data) {
            return None;
        }
        let section = self.type_section(data)?;
        let (lo, hi) = section.outline.bbox();
        let size = hi - lo;
        if !(size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0) {
            return None;
        }
        let mut out = data.clone();
        match &mut out {
            TypeData::Column { section, .. } => {
                if matches!(section, ColumnSection::Rectangular { width, depth }
                    if (*width - size.x).abs() < TOL && (*depth - size.y).abs() < TOL)
                {
                    return None;
                }
                *section = ColumnSection::Rectangular { width: size.x, depth: size.y };
            }
            TypeData::Beam { width, height, shape, .. } => {
                if *shape == SectionShape::Solid && (*width - size.x).abs() < TOL && (*height - size.y).abs() < TOL {
                    return None;
                }
                *width = size.x;
                *height = size.y;
                *shape = SectionShape::Solid;
            }
            _ => return None,
        }
        Some(out)
    }

    /// Rewrite the legacy section fields of every type that points to a
    /// valid profile as its box (see [`Self::profile_box`]), so what reads
    /// them (beam depth, axes) cannot disagree with the profile in a loaded
    /// file. Returns the number of types changed.
    pub(crate) fn resync_profile_boxes(&mut self) -> usize {
        let fixes: Vec<(ElementTypeId, TypeData)> =
            self.types.values().filter_map(|t| Some((t.id, self.profile_box(&t.data)?))).collect();
        let n = fixes.len();
        for (id, data) in fixes {
            if let Some(t) = self.types.get_mut(&id) {
                t.data = data;
            }
        }
        n
    }

    /// Whether a type's legacy section is exactly its profile (unturned,
    /// unmirrored), so the legacy fields need no rewrite.
    pub(crate) fn legacy_is_profile(&self, data: &TypeData) -> bool {
        let Some(s) = section_data(data) else { return false };
        let Some(def) = s.profile.and_then(|id| self.profile(id)) else { return false };
        s.rotation_deg == 0.0 && !s.mirror && def.shape == s.legacy
    }

    /// Point every column and beam type without a profile to one made from
    /// its legacy section; identical (material, shape) pairs share one
    /// profile, named by [`section_name`]. Types whose section makes no valid
    /// outline are left as they are. Returns the number of types changed.
    /// `translate` is the template's key lookup (the names are made of
    /// language-neutral symbols, so it is not needed yet).
    pub fn convert_sections_to_profiles(&mut self, _translate: &dyn Fn(&str) -> String) -> usize {
        let pending: Vec<ElementTypeId> = self
            .types
            .values()
            .filter(|t| section_data(&t.data).is_some_and(|s| s.profile.is_none()))
            .map(|t| t.id)
            .collect();
        let mut changed = 0;
        for id in pending {
            let Some(pid) = self
                .types
                .get(&id)
                .and_then(|t| self.legacy_profile(&t.data))
                .and_then(|(material, shape)| self.section_profile(material, shape))
            else {
                continue;
            };
            if let Some(TypeData::Column { profile, .. } | TypeData::Beam { profile, .. }) =
                self.types.get_mut(&id).map(|t| &mut t.data)
            {
                *profile = Some(pid);
                changed += 1;
            }
        }
        changed
    }

    /// The (material, shape) of a column or beam type's legacy section, when
    /// it makes a valid outline.
    pub(crate) fn legacy_profile(&self, data: &TypeData) -> Option<(ProfileMaterial, ProfileShape)> {
        let s = section_data(data)?;
        s.legacy.outline().ok()?;
        Some((self.profile_material_for(s.material), s.legacy))
    }

    /// The profile of `material` and `shape`: an existing one equal to it, or
    /// a new one named by [`section_name`] (a fresh id and uid). `None` when
    /// the project has no room for another.
    pub(crate) fn section_profile(&mut self, material: ProfileMaterial, shape: ProfileShape) -> Option<ProfileId> {
        if let Some(p) = self.profiles.values().find(|p| p.material == material && p.shape == shape) {
            return Some(p.id);
        }
        if self.profiles.len() >= MAX_PROFILES || self.next_id() == u64::MAX {
            return None;
        }
        let pid = ProfileId(self.alloc_id());
        if self.profiles.contains_key(&pid) {
            return None;
        }
        let name = self.free_profile_name(&section_name_in(&shape, material, self.measurement_system()));
        let def = ProfileDef {
            id: pid,
            uid: Uid::generate(),
            name,
            material,
            shape,
            catalog: None,
            description: String::new(),
        };
        self.profiles.insert(pid, def);
        Some(pid)
    }

    /// Clear references to profiles that are gone and turns that are not
    /// numbers; returns the number of fixes.
    pub(crate) fn repair_type_profiles(&mut self) -> usize {
        let mut repairs = 0;
        let profiles = &self.profiles;
        for t in self.types.values_mut() {
            if let TypeData::Column { profile, profile_rotation_deg, .. }
            | TypeData::Beam { profile, profile_rotation_deg, .. } = &mut t.data
            {
                if profile.is_some_and(|id| !profiles.contains_key(&id)) {
                    *profile = None;
                    repairs += 1;
                }
                if !profile_rotation_deg.is_finite() {
                    *profile_rotation_deg = 0.0;
                    repairs += 1;
                }
            }
        }
        repairs
    }

    /// `base`, or `base 2`, `base 3`… the first name no profile has.
    pub fn free_profile_name(&self, base: &str) -> String {
        free_name(base, &|n| self.profile_named(n).is_some())
    }

    /// Fix the project's profiles after loading; returns the number of fixes.
    pub(crate) fn repair_profiles(&mut self) -> usize {
        let mut repairs = 0;
        if self.profiles.len() > MAX_PROFILES {
            repairs += self.profiles.len() - MAX_PROFILES;
            let keep: BTreeSet<ProfileId> = self.profiles.keys().take(MAX_PROFILES).copied().collect();
            self.profiles.retain(|k, _| keep.contains(k));
        }
        let before = self.profiles.len();
        self.profiles.retain(|_, p| p.outline().is_ok());
        repairs += before - self.profiles.len();

        let mut names: BTreeSet<String> = BTreeSet::new();
        let mut uids: BTreeSet<Uid> = BTreeSet::new();
        for (id, p) in self.profiles.iter_mut() {
            repairs += p.repair(*id);
            if !names.insert(p.name.to_lowercase()) {
                p.name = free_name(&p.name, &|n| names.contains(&n.to_lowercase()));
                names.insert(p.name.to_lowercase());
                repairs += 1;
            }
            if !uids.insert(p.uid) {
                p.uid = Uid::generate();
                uids.insert(p.uid);
                repairs += 1;
            }
        }
        repairs
    }
}

#[cfg(test)]
mod tests {
    use crate::structural_profile::*;
    use crate::{Document, ProfileId, Project};

    fn def(id: u64, name: &str) -> ProfileDef {
        ProfileDef {
            id: ProfileId(id),
            uid: crate::Uid::generate(),
            name: name.into(),
            material: ProfileMaterial::Steel,
            shape: ProfileShape::default_for("i").unwrap(),
            catalog: None,
            description: String::new(),
        }
    }

    #[test]
    fn profiles_are_undone_as_settings() {
        let mut doc = Document::new(Project::from_template());
        doc.edit("add", |ctx| {
            let id = ProfileId(ctx.alloc_id());
            ctx.profiles_mut().insert(id, def(id.0, "IPE 200"));
        });
        assert!(doc.project.profile_named("ipe 200").is_some());
        doc.undo();
        assert!(doc.project.profile_named("IPE 200").is_none());
    }

    #[test]
    fn repair_fixes_ids_names_and_bad_shapes() {
        let mut p = Project::from_template();
        let base = p.max_id() + 1;
        // The template's own profiles are valid and stay.
        let template = p.profiles.len();
        p.profiles.insert(ProfileId(base), def(999, "A"));
        p.profiles.insert(ProfileId(base + 1), def(base + 1, "a"));
        let mut bad = def(base + 2, "Bad");
        bad.shape = ProfileShape::Rect { b: f64::NAN, h: 1.0, chamfer: 0.0 };
        p.profiles.insert(ProfileId(base + 2), bad);
        assert!(p.repair_profiles() >= 3);
        assert_eq!(p.profiles.len(), template + 2);
        assert_eq!(p.profiles[&ProfileId(base)].id, ProfileId(base));
        assert_ne!(
            p.profiles[&ProfileId(base)].name.to_lowercase(),
            p.profiles[&ProfileId(base + 1)].name.to_lowercase()
        );
        assert_eq!(p.repair_profiles(), 0);
    }

    #[test]
    fn repair_fixes_nil_and_duplicate_uids_and_empty_names() {
        let mut p = Project::empty();
        let uid = crate::Uid::generate();
        let mut a = def(1, "  ");
        a.uid = uid;
        let mut b = def(2, "B");
        b.uid = uid;
        let mut c = def(3, "C");
        c.uid = crate::Uid::default();
        for d in [a, b, c] {
            p.profiles.insert(d.id, d);
        }
        assert!(p.repair_profiles() >= 3);
        assert_eq!(p.profiles[&ProfileId(1)].name, "Profile 1");
        let uids: std::collections::BTreeSet<_> = p.profiles.values().map(|d| d.uid).collect();
        assert_eq!(uids.len(), 3);
        assert!(p.profiles.values().all(|d| !d.uid.is_nil()));
        assert_eq!(p.repair_profiles(), 0);
    }

    #[test]
    fn template_types_point_to_profiles() {
        let p = Project::from_template();
        for t in p.types.values() {
            if let crate::TypeData::Column { profile, .. } | crate::TypeData::Beam { profile, .. } = &t.data {
                let id = profile.expect("template column/beam types get a profile");
                assert!(p.profile(id).is_some());
                assert!(p.type_section(&t.data).is_some());
            }
        }
        let n = p.profiles.len();
        let mut q = p.clone();
        assert_eq!(q.convert_sections_to_profiles(&|k| k.to_string()), 0, "idempotent");
        assert_eq!(q.profiles.len(), n);
    }

    #[test]
    fn identical_sections_share_one_profile() {
        let mut p = Project::from_template();
        let column = p.types.values().find(|t| matches!(t.data, crate::TypeData::Column { .. })).unwrap().clone();
        let mut copy = column.clone();
        if let crate::TypeData::Column { profile, .. } = &mut copy.data {
            *profile = None;
        }
        copy.name = "Copy".into();
        let id = crate::ElementTypeId(p.alloc_id());
        copy.id = id;
        copy.uid = crate::Uid::generate();
        p.types.insert(id, copy);
        let before = p.profiles.len();
        assert_eq!(p.convert_sections_to_profiles(&|k| k.to_string()), 1);
        assert_eq!(p.profiles.len(), before);
        let same = |t: &crate::ElementType| match &t.data {
            crate::TypeData::Column { profile, .. } => *profile,
            _ => None,
        };
        assert_eq!(same(&p.types[&id]), same(&column));
        assert_eq!(p.profile_users(same(&column).unwrap()).len(), 2);
    }

    #[test]
    fn editing_a_profile_resizes_its_types_in_one_undo_step() {
        let mut doc = Document::new(Project::from_template());
        let (tid, pid) = doc
            .project
            .types
            .values()
            .find_map(|t| match &t.data {
                crate::TypeData::Beam { profile: Some(p), .. } => Some((t.id, *p)),
                _ => None,
            })
            .unwrap();
        doc.edit("grow", |ctx| {
            let p = ctx.profiles_mut().get_mut(&pid).unwrap();
            p.shape = ProfileShape::Rect { b: 0.25, h: 0.6, chamfer: 0.0 };
        });
        let crate::TypeData::Beam { width, height, .. } = doc.project.types[&tid].data else { panic!() };
        assert!((width - 0.25).abs() < 1e-9 && (height - 0.6).abs() < 1e-9);
        doc.undo();
        let crate::TypeData::Beam { height, .. } = doc.project.types[&tid].data else { panic!() };
        assert!((height - 0.6).abs() > 1e-6);
    }

    #[test]
    fn steel_column_converts_to_a_named_i_profile() {
        let p = Project::from_template();
        let i = p.profiles.values().find(|d| matches!(d.shape, ProfileShape::I { .. })).unwrap();
        assert_eq!(i.material, ProfileMaterial::Steel);
        assert!(p.profile_named("I 200x200x9x15").is_some(), "{:?}", p.profiles.values().map(|d| &d.name));
        assert!(p.profile_named("Rect 20x20").is_some());
        assert!(p.profile_named("Ø30").is_some());
        assert!(p.profile_named("Tube 150x150x6").is_some());
    }

    #[test]
    fn unrelated_edits_keep_legacy_sections_that_are_their_profile() {
        let mut doc = Document::new(Project::from_template());
        let before = doc.project.types.clone();
        doc.edit("units", |ctx| {
            let _ = ctx.profiles_mut();
        });
        assert_eq!(doc.project.types, before);
    }

    #[test]
    fn turned_and_mirrored_sections() {
        let mut p = Project::from_template();
        let (tid, pid) = p
            .types
            .values()
            .find_map(|t| match &t.data {
                crate::TypeData::Beam { profile: Some(id), .. } => Some((t.id, *id)),
                _ => None,
            })
            .unwrap();
        p.profiles.get_mut(&pid).unwrap().shape = ProfileShape::Angle { h: 0.2, b: 0.1, t: 0.01, r: 0.0 };
        let plain = p.type_section(&p.types[&tid].data).unwrap();
        if let crate::TypeData::Beam { profile_rotation_deg, profile_mirror, .. } =
            &mut p.types.get_mut(&tid).unwrap().data
        {
            *profile_rotation_deg = 90.0;
            *profile_mirror = true;
        }
        let s = p.type_section(&p.types[&tid].data).unwrap();
        let (lo, hi) = s.outline.bbox();
        assert!(((hi - lo) - glam::DVec2::new(0.2, 0.1)).abs().max_element() < 1e-9);
        let area = |o: &SectionOutline| {
            let r = &o.outer;
            0.5 * (0..r.len()).map(|i| r[i].perp_dot(r[(i + 1) % r.len()])).sum::<f64>()
        };
        assert!(
            (area(&s.outline) - area(&plain.outline)).abs() < 1e-12 && area(&s.outline) > 0.0,
            "stays counterclockwise"
        );
    }

    #[test]
    fn dangling_profile_is_dropped_by_the_next_edit_keeping_the_box() {
        let mut doc = Document::new(Project::from_template());
        let (tid, pid) = doc
            .project
            .types
            .values()
            .find_map(|t| match &t.data {
                crate::TypeData::Column { profile: Some(id), .. } => Some((t.id, *id)),
                _ => None,
            })
            .unwrap();
        let size = crate::profiles_project::legacy_size(&doc.project.types[&tid].data);
        doc.edit("remove", |ctx| {
            ctx.profiles_mut().remove(&pid);
        });
        let data = &doc.project.types[&tid].data;
        assert!(matches!(data, crate::TypeData::Column { profile: None, .. }));
        assert_eq!(crate::profiles_project::legacy_size(data), size);
        assert!(doc.project.type_section(data).is_some());
    }

    #[test]
    fn a_legacy_edit_repoints_but_a_new_profile_wins() {
        let mut doc = Document::new(Project::from_template());
        let (tid, old) = doc
            .project
            .types
            .values()
            .find_map(|t| match &t.data {
                crate::TypeData::Column {
                    profile: Some(id),
                    section: crate::ColumnSection::Rectangular { .. },
                    ..
                } => Some((t.id, *id)),
                _ => None,
            })
            .unwrap();
        let round = doc.project.profiles.values().find(|d| matches!(d.shape, ProfileShape::Round { .. })).unwrap().id;
        // The section alone: an existing equal profile is reused.
        doc.edit("round", |ctx| {
            if let crate::TypeData::Column { section, .. } = &mut ctx.type_mut(tid).unwrap().data {
                *section = crate::ColumnSection::Round { diameter: 0.3 };
            }
        });
        let profile = |doc: &Document| type_profile_of(&doc.project.types[&tid].data);
        assert_eq!(profile(&doc), Some(round));
        // Both: the profile wins and the section becomes its box.
        doc.edit("both", |ctx| {
            if let crate::TypeData::Column { section, profile, .. } = &mut ctx.type_mut(tid).unwrap().data {
                *section = crate::ColumnSection::Round { diameter: 0.5 };
                *profile = Some(old);
            }
        });
        assert_eq!(profile(&doc), Some(old));
        let crate::TypeData::Column { section, .. } = doc.project.types[&tid].data else { panic!() };
        assert_eq!(section.size(), (0.2, 0.2));
    }

    fn type_profile_of(data: &crate::TypeData) -> Option<ProfileId> {
        crate::profiles_project::type_profile(data)
    }

    #[test]
    fn free_names_count_up() {
        let mut p = Project::empty();
        p.profiles.insert(ProfileId(1), def(1, "W 150 x 13"));
        assert_eq!(p.free_profile_name("W 150 x 13"), "W 150 x 13 2");
        assert_eq!(p.free_profile_name("HEB 200"), "HEB 200");
    }

    #[test]
    fn oversized_sketch_is_refused_before_expansion() {
        use crate::profile::ProfilePoint;
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline = (0..MAX_POINTS + 1).map(|_| ProfilePoint::new("0", "0")).collect();
        assert_eq!(sk.resolve().unwrap_err(), ProfileError::TooBig);
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.openings = vec![(0..MAX_POINTS).map(|_| ProfilePoint::new("0", "0")).collect()];
        assert_eq!(sk.resolve().unwrap_err(), ProfileError::TooBig);
    }

    #[test]
    fn sketch_errors_read_as_sentences() {
        use crate::profile::ProfilePoint;
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline[0] = ProfilePoint::new("nope", "0");
        let msg = sk.resolve().unwrap_err().to_string();
        assert!(!msg.contains('{') && !msg.contains('}'), "{msg}");
        assert!(msg.contains("nope"), "{msg}");
    }

    fn loop_of(pts: &[(f64, f64)]) -> Vec<crate::profile::ProfilePoint> {
        pts.iter().map(|(x, y)| crate::profile::ProfilePoint::new(x.to_string(), y.to_string())).collect()
    }

    fn sketch_err(sk: &ParamSketch) -> String {
        match sk.resolve() {
            Err(ProfileError::Sketch(m)) => m,
            other => panic!("expected a sketch error, got {other:?}"),
        }
    }

    #[test]
    fn crossing_sketches_are_refused() {
        // A bow-tie with unequal lobes (its signed area is not zero).
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline = loop_of(&[(0.0, 0.0), (0.3, 0.2), (0.3, 0.0), (0.0, 0.1)]);
        assert!(sketch_err(&sk).contains("crosses itself"));

        // A U whose opening bridges the notch: every corner of the opening is
        // inside the outline, but its edges cross it.
        let u = [(0.0, 0.0), (0.3, 0.0), (0.3, 0.3), (0.2, 0.3), (0.2, 0.1), (0.1, 0.1), (0.1, 0.3), (0.0, 0.3)];
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline = loop_of(&u);
        sk.openings = vec![loop_of(&[(0.05, 0.2), (0.25, 0.2), (0.25, 0.25), (0.05, 0.25)])];
        assert!(sketch_err(&sk).contains("crosses the outline"));

        // Two overlapping openings, and one inside another.
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline = loop_of(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        sk.openings = vec![
            loop_of(&[(0.1, 0.1), (0.5, 0.1), (0.5, 0.5), (0.1, 0.5)]),
            loop_of(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.7), (0.3, 0.7)]),
        ];
        assert!(sketch_err(&sk).contains("openings overlap"));
        sk.openings[1] = loop_of(&[(0.2, 0.2), (0.3, 0.2), (0.3, 0.3), (0.2, 0.3)]);
        assert!(sketch_err(&sk).contains("openings overlap"));

        // Apart, they make a section.
        sk.openings[1] = loop_of(&[(0.6, 0.6), (0.9, 0.6), (0.9, 0.9), (0.6, 0.9)]);
        assert!(sk.resolve().is_ok());
    }

    #[test]
    fn repair_drops_a_crossing_sketch_profile() {
        let mut p = Project::from_template();
        let base = p.max_id() + 1;
        let mut bad = def(base, "Bow-tie");
        let mut sk = ParamSketch::square(0.1, 0.1);
        sk.outline = loop_of(&[(0.0, 0.0), (0.3, 0.2), (0.3, 0.0), (0.0, 0.1)]);
        bad.shape = ProfileShape::Sketch(sk);
        p.profiles.insert(ProfileId(base), bad);
        let ron = crate::persist::to_ron(&p).unwrap();
        let (loaded, repairs) = crate::persist::from_ron_with_repairs(&ron).unwrap();
        assert!(repairs >= 1);
        assert!(loaded.profile(ProfileId(base)).is_none());
        assert_eq!(loaded.profiles.len(), p.profiles.len() - 1);
    }
}
