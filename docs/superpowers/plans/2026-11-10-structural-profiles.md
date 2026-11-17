# Structural Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make structural cross-sections first-class, named, parametric project objects (`Project::profiles`). Build them from standard shapes or a parametric sketch, import them from built-in steel, cold-formed, timber and tube catalogs, and use them in column and beam types through geometry, IFC, the engine and a Profile Manager in the app.

**Architecture:** A new `cc_core::structural_profile` module holds `ProfileDef` (standard `ProfileShape` or `ParamSketch`), outline resolution and section properties. Profiles are tracked project settings, like fill patterns.

Column and beam types gain `profile: Option<ProfileId>`. The legacy section fields stay in files as a *bounding-box cache*, which `EditCtx::finish → sync_profiles` keeps in step, so the code that only needs a size keeps working. Geometry, IFC and properties go through one resolver, `Project::type_section`.

On load, `convert_sections_to_profiles` turns every legacy section into a deduplicated profile (format 27). Catalogs are CSV assets compiled in with `include_str!`. Engine commands live in `cc_engine::commands::profiles`, and the app's `profiles_dialog.rs` calls them.

**Tech Stack:** Rust 2024 workspace, serde/RON, schemars, glam `DVec2`, `earcutr` (via `cc_geom::polygon`), egui/eframe, egui_kittest, Fluent i18n.

**Spec:** `docs/superpowers/specs/2026-11-10-structural-profiles-design.md`

## Global Constraints

- Work in the worktree `D:\constructcad-profiles`, branch `feat/structural-profiles`. Never touch `D:\constructcad`, where other agents may be working.
- Commit with `git commit -s` (DCO). Code, comments, identifiers and commit messages are in English.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, and no `unsafe`. Errors are values.
- Files are hostile input: every new field has a serde default, every loop over file data is bounded, and divisions are guarded against 0/NaN.
- `FORMAT_VERSION` goes from 26 to 27. New fields have `#[serde(default)]` so older files load.
- All lengths are in meters in the model and commands; the CSV catalogs are in millimeters.
- Every new interface string uses an i18n key added to all 12 `assets/i18n/*.ftl` files (`pt-BR` is the reference).
- Format with `cargo fmt --all` (max_width 120). `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- Every user-facing operation is an engine command with a test in `crates/cc_engine/tests/profiles.rs`. Every command parameter has a doc comment, and `docs/commands.md` is regenerated.
- Neutral terminology (Profile, Type, Component) and our own SVG icons. Catalog rows are dimensions from public tables only.
- Spec deviation, approved by the planner: legacy section fields remain *written* as a bounding-box cache, kept in sync by `sync_profiles`, rather than "no longer written". Task 5 updates the spec to say so.

## File Structure

| File | Responsibility |
|---|---|
| `crates/cc_core/src/structural_profile.rs` (new) | `ProfileId` usage, `ProfileMaterial`, `ProfileShape`, `ParamSketch`, `ProfileDef`, `SectionOutline`, `outline()`, limits, repair of one profile |
| `crates/cc_core/src/section_props.rs` (new) | `SectionProperties` and polygon integration |
| `crates/cc_core/src/profile_catalog.rs` (new) | CSV parsing, `Series`, `CatalogRow`, `catalog()`, `row_to_def` |
| `crates/cc_core/assets/profiles/*.csv` (new) | Built-in catalog data |
| `crates/cc_core/src/profiles_project.rs` (new) | `impl Project`: lookup, users, `type_section`, `convert_sections_to_profiles`, `repair_profiles` |
| `crates/cc_core/src/{ids,lib,project,document,persist,repair,types}.rs` | Wiring: id, field, settings snapshot, `profiles_mut`, `sync_profiles`, format 27, repair call, type fields |
| `crates/cc_geom/src/section.rs` | `TypeSection`-driven column parts and loops, detail levels |
| `crates/cc_geom/src/beam.rs`, `build.rs` | Beams swept from general section polygons |
| `crates/cc_views/src/annotate.rs`, `schedule.rs` | Tags show the profile name; mass per length |
| `crates/cc_engine/src/commands/profiles.rs` (new) | Every profile command |
| `crates/cc_engine/tests/profiles.rs` (new) | Engine tests |
| `crates/cc_ifc/src/export.rs`, `import.rs`, `tests/round_trip.rs`, `tests/fixtures/profiles.ccad` | IFC |
| `crates/cc_app/src/profiles_dialog.rs` (new), `commands.rs`, `ribbon.rs`, `type_dialog.rs`, `sketch_mode.rs`, `assets/icons/ui/profiles.svg`, `assets/i18n/*.ftl` | App |
| `docs/automation.md`, `docs/automacao.md`, `docs/commands.md`, `AGENTS.md`, `.github/workflows/ci.yml` | Documentation and CI |

---

### Task 1: Standard profile shapes and their outlines

**Files:**
- Create: `crates/cc_core/src/structural_profile.rs`
- Modify: `crates/cc_core/src/ids.rs` (add `id_type!(ProfileId, "PR");` next to `FillPatternId`)
- Modify: `crates/cc_core/src/lib.rs` (`pub mod structural_profile;`, re-export `ProfileId` in the `ids` use list and `pub use structural_profile::{ProfileDef, ProfileMaterial, ProfileShape, SectionOutline};`)

**Interfaces:**
- Produces: `ProfileId`, `ProfileMaterial { Steel, ColdFormed, Timber, Concrete }`, `ProfileShape` (16 variants, all `f64` fields in meters), `ProfileDef { id, uid, name, material, shape, catalog: Option<CatalogRef>, description }`, `CatalogRef { series: String, designation: String }`, `SectionOutline { outer: Vec<DVec2>, holes: Vec<Vec<DVec2>> }` (outer CCW, holes CW, centered on the bounding-box center), `ProfileError` (thiserror), `ProfileDef::outline(&self) -> Result<SectionOutline, ProfileError>`, `ProfileShape::outline(&self) -> Result<SectionOutline, ProfileError>`, `ProfileShape::code(&self) -> &'static str`, `ProfileShape::params(&self) -> Vec<(&'static str, f64)>`, `ProfileShape::set_param(&mut self, name: &str, v: f64) -> bool`, `ProfileShape::default_for(code: &str) -> Option<ProfileShape>`, `SectionOutline::bbox(&self) -> (DVec2, DVec2)`, constants `ARC_SEGMENTS: usize = 8` (per quarter circle), `MAX_POINTS: usize = 4096`, `MAX_OPENINGS: usize = 64`, `MIN_THICKNESS: f64 = 0.0003`, `MAX_SIZE: f64 = 10.0`, `MAX_NAME: usize = 100`, `MAX_PROFILES: usize = 5000`.

- [ ] **Step 1: Write the failing tests** at the bottom of `structural_profile.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn area(o: &SectionOutline) -> f64 {
        let a = |p: &[DVec2]| 0.5 * (0..p.len()).map(|i| p[i].perp_dot(p[(i + 1) % p.len()])).sum::<f64>();
        a(&o.outer) + o.holes.iter().map(|h| a(h)).sum::<f64>()
    }

    #[test]
    fn rect_and_round_areas() {
        let r = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 }.outline().unwrap();
        assert!((area(&r) - 0.08).abs() < 1e-12);
        let c = ProfileShape::Round { d: 0.3 }.outline().unwrap();
        let exact = std::f64::consts::PI * 0.15 * 0.15;
        assert!((area(&c) - exact).abs() / exact < 0.01);
    }

    #[test]
    fn i_without_root_radius_is_exact() {
        // IPE-like: d 200, bf 100, tw 5.6, tf 8.5, r 0
        let o = ProfileShape::I { d: 0.2, bf: 0.1, tw: 0.0056, tf: 0.0085, r: 0.0 }.outline().unwrap();
        let exact = 2.0 * 0.1 * 0.0085 + (0.2 - 2.0 * 0.0085) * 0.0056;
        assert!((area(&o) - exact).abs() < 1e-12);
        let (lo, hi) = o.bbox();
        assert!((hi - lo - DVec2::new(0.1, 0.2)).length() < 1e-12);
    }

    #[test]
    fn root_radius_adds_four_fillets() {
        let (d, bf, tw, tf, r) = (0.2, 0.1, 0.0056, 0.0085, 0.012);
        let o = ProfileShape::I { d, bf, tw, tf, r }.outline().unwrap();
        let plain = 2.0 * bf * tf + (d - 2.0 * tf) * tw;
        let fillets = 4.0 * (1.0 - std::f64::consts::FRAC_PI_4) * r * r;
        assert!((area(&o) - (plain + fillets)).abs() / (plain + fillets) < 0.002);
    }

    #[test]
    fn tubes_have_holes() {
        let o = ProfileShape::RectTube { h: 0.1, b: 0.05, t: 0.004, ro: 0.0 }.outline().unwrap();
        assert_eq!(o.holes.len(), 1);
        assert!((area(&o) - (0.1 * 0.05 - 0.092 * 0.042)).abs() < 1e-12);
        let c = ProfileShape::RoundTube { d: 0.1, t: 0.005 }.outline().unwrap();
        assert_eq!(c.holes.len(), 1);
    }

    #[test]
    fn lipped_channel_area_matches_centerline_formula() {
        // Ue 90x40x12x0.95 with ri 0: area = t * (d + 2 bf + 2 lip - 4 t) for square corners
        let (d, bf, lip, t) = (0.09, 0.04, 0.012, 0.00095);
        let o = ProfileShape::ColdLippedU { d, bf, lip, t, ri: 0.0 }.outline().unwrap();
        let exact = t * (d + 2.0 * bf + 2.0 * lip - 4.0 * t);
        assert!((area(&o) - exact).abs() / exact < 1e-9);
    }

    #[test]
    fn every_default_shape_resolves() {
        for code in ProfileShape::CODES {
            let s = ProfileShape::default_for(code).unwrap();
            assert!(s.outline().is_ok(), "{code}");
            assert_eq!(s.code(), *code);
        }
    }

    #[test]
    fn bad_values_are_errors_not_panics() {
        assert!(ProfileShape::I { d: 0.2, bf: 0.1, tw: 0.2, tf: 0.0085, r: 0.0 }.outline().is_err());
        assert!(ProfileShape::Rect { b: f64::NAN, h: 0.4, chamfer: 0.0 }.outline().is_err());
        assert!(ProfileShape::RoundTube { d: 0.1, t: 0.06 }.outline().is_err());
        assert!(ProfileShape::Rect { b: 50.0, h: 0.4, chamfer: 0.0 }.outline().is_err());
    }

    #[test]
    fn set_param_round_trips() {
        let mut s = ProfileShape::default_for("i").unwrap();
        assert!(s.set_param("tw", 0.007));
        assert!(!s.set_param("nope", 1.0));
        assert_eq!(s.params().iter().find(|(n, _)| *n == "tw").unwrap().1, 0.007);
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cc_core structural_profile`
Expected: compile error (module missing).

- [ ] **Step 3: Implement.** Module skeleton (fill in every variant following these rules):

```rust
//! Structural profiles: the cross-sections of columns, beams and framing
//! members as named project objects (`Project::profiles`). A profile is a
//! standard parametric shape (rolled, welded, cold-formed, timber or
//! concrete) or a parametric sketch ([`ParamSketch`], see Task 2). Lengths
//! are meters. Outlines are laid out as in steel tables: the depth along
//! +y, the web of an I or channel vertical, the flanges of a channel and
//! the legs of an angle pointing to +x; centered on the bounding box.

use crate::{ProfileId, Uid};
use glam::DVec2;
use serde::{Deserialize, Serialize};

pub const ARC_SEGMENTS: usize = 8;
pub const MAX_POINTS: usize = 4096;
pub const MAX_OPENINGS: usize = 64;
pub const MIN_THICKNESS: f64 = 0.0003;
pub const MAX_SIZE: f64 = 10.0;
pub const MAX_NAME: usize = 100;
pub const MAX_DESCRIPTION: usize = 200;
pub const MAX_PROFILES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ProfileMaterial { #[default] Steel, ColdFormed, Timber, Concrete }

impl ProfileMaterial {
    pub const ALL: [ProfileMaterial; 4] = [Self::Steel, Self::ColdFormed, Self::Timber, Self::Concrete];
    pub fn code(self) -> &'static str { match self { Self::Steel => "steel", Self::ColdFormed => "cold_formed", Self::Timber => "timber", Self::Concrete => "concrete" } }
    pub fn from_code(c: &str) -> Option<Self> { Self::ALL.into_iter().find(|m| m.code().eq_ignore_ascii_case(c.trim())) }
    pub fn key(self) -> &'static str { match self { Self::Steel => "profile-material-steel", Self::ColdFormed => "profile-material-cold-formed", Self::Timber => "profile-material-timber", Self::Concrete => "profile-material-concrete" } }
    /// The `MaterialClass` a type made for this profile gets its material from.
    pub fn material_class(self) -> crate::MaterialClass { match self { Self::Steel | Self::ColdFormed => crate::MaterialClass::Metal, Self::Timber => crate::MaterialClass::Wood, Self::Concrete => crate::MaterialClass::Concrete } }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProfileShape {
    I { d: f64, bf: f64, tw: f64, tf: f64, r: f64 },
    IAsym { d: f64, bf_top: f64, tf_top: f64, bf_bot: f64, tf_bot: f64, tw: f64 },
    Channel { d: f64, bf: f64, tw: f64, tf: f64, r: f64 },
    T { d: f64, bf: f64, tw: f64, tf: f64, r: f64 },
    Angle { h: f64, b: f64, t: f64, r: f64 },
    ColdU { d: f64, bf: f64, t: f64, ri: f64 },
    ColdLippedU { d: f64, bf: f64, lip: f64, t: f64, ri: f64 },
    ColdZ { d: f64, bf: f64, lip: f64, t: f64, ri: f64, angle: f64 },
    ColdHat { d: f64, bf: f64, lip: f64, t: f64, ri: f64 },
    RectTube { h: f64, b: f64, t: f64, ro: f64 },
    RoundTube { d: f64, t: f64 },
    Rect { b: f64, h: f64, chamfer: f64 },
    Round { d: f64 },
    ConcreteT { h: f64, bf: f64, hf: f64, bw: f64 },
    ConcreteL { h: f64, bf: f64, hf: f64, bw: f64 },
    Sketch(ParamSketch),
}

/// Filled in by Task 2. In Task 1 it is an empty struct, and outline()
/// returns Err(ProfileError::Sketch("empty sketch".into())) for it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParamSketch {}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionOutline { pub outer: Vec<DVec2>, pub holes: Vec<Vec<DVec2>> }

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProfileError {
    #[error("{0} must be a number between {MIN_THICKNESS} and {MAX_SIZE} m")]
    Value(&'static str),
    #[error("{0}")]
    DoesNotFit(String),
    #[error("sketch: {0}")]
    Sketch(String),
    #[error("the outline has more than {MAX_POINTS} points")]
    TooBig,
}
```

Implementation rules:

1. **Checks before building.** Every field must be finite, at most `MAX_SIZE`, and at least `MIN_THICKNESS`. The exceptions are radii, `chamfer` and `angle`, which may be 0, and `ColdZ::angle`, which must lie in 0..=90°. Then check fit: `2*tf < d`, `tw < bf`, `t < d/2`, `2*t < min(h,b)`, `2*t < d`, `r <= (bf - tw)/2`, `lip > t`. If a check fails, return `DoesNotFit` with a sentence naming the parameters.
2. **Fillets.** Add a helper `fn fillet(corner: DVec2, a: DVec2, b: DVec2, r: f64) -> Vec<DVec2>`. It returns `ARC_SEGMENTS + 1` points of the arc tangent to the two edges that leave `corner` along unit directions `a` and `b`. With r = 0 it returns `vec![corner]`. Re-entrant corners (root radius `r`, cold-formed inner `ri`) add material. Cold-formed outer corners use `ri + t` and the matching inner corners use `ri`.
3. **I and IAsym.** Build the 12 vertices as in `cc_geom::section::shape_loops` (`SectionShape::I`), with web-to-flange fillets on the 4 inner corners. For IAsym, put the centroid-independent bounding box center at the origin.
4. **Cold-formed shapes are a polyline offset.** For U, Ue, Z and Hat, build the centerline polyline with corners at the right distances. Then offset it ±t/2, using the bend radius at each corner as the arc center radius `ri + t/2`. This gives exactly t·(centerline length) when ri = 0 at square corners. The test above checks Ue.
5. **Circles** use `4 * ARC_SEGMENTS` points.
6. **Winding and bounds.** `outer` is CCW and holes are CW; normalize with the signed area. Return `TooBig` past `MAX_POINTS`.
7. **Defaults.** `CODES: &[&str] = &["i","i_asym","channel","t","angle","cold_u","cold_lipped_u","cold_z","cold_hat","rect_tube","round_tube","rect","round","concrete_t","concrete_l","sketch"]`. `default_for` gives sensible values (I = IPE 200, Ue = 90x40x12x0.95, Rect = 0.15x0.30, Sketch = a 0.1 m square with params `b`/`h`, see Task 2).
8. **Parameters.** `params()`/`set_param()` list each variant's fields by their spec names, in declaration order. Sketch returns its `ParamSketch::params` values.
9. **`ProfileDef`.** Fields `id: ProfileId, uid: Uid, name: String, material: ProfileMaterial, shape: ProfileShape, catalog: Option<CatalogRef>, description: String`; derive `Debug, Clone, PartialEq, Serialize, Deserialize`. The fields `#[serde(default)] uid: Uid`, `catalog: Option<CatalogRef>` and `description: String` have serde defaults. `outline()` forwards to `shape.outline()`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core structural_profile`
Expected: all 8 pass.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p cc_core --all-targets -- -D warnings
git add crates/cc_core && git commit -s -m "Add parametric structural profile shapes"
```

---

### Task 2: Parametric sketch profiles

**Files:**
- Modify: `crates/cc_core/src/structural_profile.rs`

**Interfaces:**
- Consumes: `crate::profile::{ProfilePoint, outline}` (`outline(points, lookup) -> Result<Vec<DVec2>, profile::ProfileError>`), `crate::component::{ComponentParam, ParamKind}`, `crate::expr`.
- Produces: `ParamSketch { params: Vec<ComponentParam>, outline: Vec<ProfilePoint>, openings: Vec<Vec<ProfilePoint>> }` (Default, Serialize, Deserialize, PartialEq, Clone, Debug), `ParamSketch::resolve(&self) -> Result<SectionOutline, ProfileError>`, `ParamSketch::square(b: f64, h: f64) -> ParamSketch`, and the constant `MAX_SKETCH_PARAMS: usize = 64`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn sketch_reads_its_parameters() {
    let mut s = ParamSketch::square(0.1, 0.2);
    let o = ProfileShape::Sketch(s.clone()).outline().unwrap();
    let (lo, hi) = o.bbox();
    assert!((hi - lo - DVec2::new(0.1, 0.2)).length() < 1e-12);
    s.params.iter_mut().find(|p| p.name == "b").unwrap().value = 0.3;
    let (lo, hi) = ProfileShape::Sketch(s).outline().unwrap().bbox();
    assert!((hi.x - lo.x - 0.3).abs() < 1e-12);
}

#[test]
fn sketch_with_opening_is_a_hollow_section() {
    let mut s = ParamSketch::square(0.2, 0.2);
    s.params.push(crate::component::ComponentParam::number("t", 0.01, crate::component::ParamKind::Length));
    s.openings.push(vec![
        ProfilePoint::new("-b/2+t", "-h/2+t"), ProfilePoint::new("b/2-t", "-h/2+t"),
        ProfilePoint::new("b/2-t", "h/2-t"), ProfilePoint::new("-b/2+t", "h/2-t"),
    ]);
    let o = ProfileShape::Sketch(s).outline().unwrap();
    assert_eq!(o.holes.len(), 1);
}

#[test]
fn bad_sketch_is_an_error() {
    let mut s = ParamSketch::square(0.1, 0.1);
    s.outline[0].u = "nope + 1".into();
    assert!(matches!(ProfileShape::Sketch(s).outline(), Err(ProfileError::Sketch(_))));
    let mut s = ParamSketch::square(0.1, 0.1);
    s.outline.truncate(2);
    assert!(ProfileShape::Sketch(s).outline().is_err());
}
```

- [ ] **Step 2: Run** `cargo test -p cc_core structural_profile`. Expected: fail (no `ParamSketch::square`).
- [ ] **Step 3: Implement.**
  - Replace the Task 1 placeholder unit struct with the real struct.
  - `square(b, h)` declares params `b` and `h` (`ParamKind::Length`) and four points `("-b/2","-h/2")`, `("b/2","-h/2")`, `("b/2","h/2")`, `("-b/2","h/2")`.
  - `resolve` builds the lookup over numeric params and calls `crate::profile::outline` for the outline and each opening. It maps any error to `ProfileError::Sketch(e.to_string())` and refuses more than `MAX_OPENINGS` openings, more than `MAX_SKETCH_PARAMS` params, or fewer than 3 points.
  - Each opening must lie inside the outline (every vertex inside, by the even-odd test). An opening outside is a `Sketch("an opening lies outside the outline")` error.
  - Normalize the winding, then translate everything so the bounding-box center is at the origin.
- [ ] **Step 4: Run** `cargo test -p cc_core structural_profile`. Expected: all pass.
- [ ] **Step 5: Commit** `git commit -s -m "Let structural profiles be parametric sketches"`

---

### Task 3: Section properties

**Files:**
- Create: `crates/cc_core/src/section_props.rs`
- Modify: `crates/cc_core/src/lib.rs` (`pub mod section_props; pub use section_props::SectionProperties;`)

**Interfaces:**
- Consumes: `SectionOutline` (Task 1).
- Produces: `SectionProperties { area, centroid: [f64;2], ix, iy, ixy, wx, wy, rx, ry, j, perimeter, linear_mass }` (all f64, SI units: m², m⁴, m³, m, kg/m), derives `Debug, Clone, Copy, PartialEq, Default, Serialize`, and `SectionOutline::properties(&self, density: f64) -> SectionProperties`. Ix and Iy are about the centroidal x/y axes. `wx = ix / max distance in y from the centroid`. `j` is approximate: for closed shapes (with holes) use Bredt's 4A²t/s on the mid-wall; for open shapes Σ b t³ / 3 over the thin-walled parts, approximated as `perimeter * t_eq³ / 6` with `t_eq = 2 * area / perimeter`. Mark `j` as approximate in its doc comment.

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use crate::structural_profile::ProfileShape;

    #[test]
    fn rectangle_closed_forms() {
        let p = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 }.outline().unwrap().properties(7850.0);
        assert!((p.area - 0.08).abs() < 1e-12);
        assert!((p.ix - 0.2 * 0.4f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((p.iy - 0.4 * 0.2f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((p.wx - 0.2 * 0.4 * 0.4 / 6.0).abs() < 1e-12);
        assert!((p.linear_mass - 0.08 * 7850.0).abs() < 1e-9);
        assert!(p.centroid[0].abs() < 1e-12 && p.centroid[1].abs() < 1e-12);
        assert!((p.perimeter - 1.2).abs() < 1e-12);
    }

    #[test]
    fn circle_and_tube() {
        let p = ProfileShape::Round { d: 0.3 }.outline().unwrap().properties(0.0);
        let exact = std::f64::consts::PI * 0.15f64.powi(4) / 4.0;
        assert!((p.ix - exact).abs() / exact < 0.02);
        let t = ProfileShape::RectTube { h: 0.1, b: 0.1, t: 0.005, ro: 0.0 }.outline().unwrap().properties(0.0);
        let exact = (0.1f64.powi(4) - 0.09f64.powi(4)) / 12.0;
        assert!((t.ix - exact).abs() / exact < 1e-9);
    }

    #[test]
    fn i_section_against_hand_calculation() {
        let (d, bf, tw, tf) = (0.2, 0.1, 0.0056, 0.0085);
        let p = ProfileShape::I { d, bf, tw, tf, r: 0.0 }.outline().unwrap().properties(0.0);
        let hw = d - 2.0 * tf;
        let ix = bf * d.powi(3) / 12.0 - (bf - tw) * hw.powi(3) / 12.0;
        assert!((p.ix - ix).abs() / ix < 1e-9);
    }

    #[test]
    fn angle_centroid_is_off_center() {
        let p = ProfileShape::Angle { h: 0.1, b: 0.1, t: 0.01, r: 0.0 }.outline().unwrap().properties(0.0);
        assert!(p.centroid[0] < 0.0 && p.centroid[1] < 0.0);
        assert!(p.ixy.abs() > 0.0);
    }
}
```

- [ ] **Step 2: Run** `cargo test -p cc_core section_props`. Expected: fail.
- [ ] **Step 3: Implement** with Green's theorem over each ring (outer CCW positive, holes CW negative):
  - per edge, `c = x_i*y_{i+1} - x_{i+1}*y_i`;
  - `A = Σc/2`;
  - `Cx = Σ(x_i+x_{i+1})c / 6A`, and the same for Cy;
  - `Ixx_origin = Σ(y_i²+y_i y_{i+1}+y_{i+1}²)c/12`;
  - `Iyy_origin = Σ(x_i²+x_i x_{i+1}+x_{i+1}²)c/12`;
  - `Ixy_origin = Σ(x_i y_{i+1}+2x_i y_i+2x_{i+1}y_{i+1}+x_{i+1}y_i)c/24`;
  - shift to the centroid with the parallel-axis theorem.

  Guard `A <= 1e-15`: return `Default`. Guard non-finite density: treat it as 0. Perimeter is the outer length plus the hole lengths.
- [ ] **Step 4: Run** the tests. Expected: pass.
- [ ] **Step 5: Commit** `git commit -s -m "Compute section properties of profile outlines"`

---

### Task 4: Profiles in the project (settings, undo, repair)

**Files:**
- Create: `crates/cc_core/src/profiles_project.rs` (`impl Project` blocks; `mod profiles_project;` in lib.rs)
- Modify: `crates/cc_core/src/project.rs`:
  - after `fill_patterns` (line ~102), add `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")] pub profiles: BTreeMap<ProfileId, ProfileDef>,`;
  - add `profiles: BTreeMap::new()` in `Project::empty`;
  - add `let profiles = self.profiles.keys().map(|k| k.0);` and `.chain(profiles)` in `max_id` (~line 969).
- Modify: `crates/cc_core/src/document.rs`:
  - add `profiles` to `Settings` (field, `of`, `restore`, lines 15-73);
  - add `pub fn profiles_mut(&mut self) -> &mut BTreeMap<ProfileId, ProfileDef>` next to `fill_patterns_mut` (line ~551).
- Modify: `crates/cc_core/src/repair.rs` (call `repairs += self.repair_profiles();` after `repair_fill_patterns`, line ~354).

**Interfaces:**
- Produces:
  - `Project::profile(&self, id) -> Option<&ProfileDef>`
  - `Project::profile_named(&self, name) -> Option<&ProfileDef>` (trim, ignoring case)
  - `Project::profile_users(&self, id) -> Vec<ElementTypeId>`
  - `Project::free_profile_name(&self, base: &str) -> String` (`base`, `base 2`, `base 3`…)
  - `EditCtx::profiles_mut()`
  - `pub(crate) fn repair_profiles(&mut self) -> usize`
  - `ProfileDef::repair(&mut self, id) -> usize`

- [ ] **Step 1: Failing tests** in `profiles_project.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::structural_profile::*;
    use crate::{Document, Project, ProfileId};

    fn def(id: u64, name: &str) -> ProfileDef {
        ProfileDef { id: ProfileId(id), uid: crate::Uid::generate(), name: name.into(), material: ProfileMaterial::Steel,
            shape: ProfileShape::default_for("i").unwrap(), catalog: None, description: String::new() }
    }

    #[test]
    fn profiles_are_undone_as_settings() {
        let mut doc = Document::new(Project::from_template());
        doc.edit("add", |ctx| { let id = ProfileId(ctx.alloc_id()); ctx.profiles_mut().insert(id, def(id.0, "IPE 200")); });
        assert!(doc.project.profile_named("ipe 200").is_some());
        doc.undo();
        assert!(doc.project.profile_named("IPE 200").is_none());
    }

    #[test]
    fn repair_fixes_ids_names_and_bad_shapes() {
        let mut p = Project::from_template();
        let base = p.max_id() + 1;
        p.profiles.insert(ProfileId(base), def(999, "A"));
        p.profiles.insert(ProfileId(base + 1), def(base + 1, "a"));
        let mut bad = def(base + 2, "Bad");
        bad.shape = ProfileShape::Rect { b: f64::NAN, h: 1.0, chamfer: 0.0 };
        p.profiles.insert(ProfileId(base + 2), bad);
        assert!(p.repair_profiles() >= 3);
        assert_eq!(p.profiles.len(), 2);
        assert_eq!(p.profiles[&ProfileId(base)].id, ProfileId(base));
        assert_ne!(p.profiles[&ProfileId(base)].name.to_lowercase(), p.profiles[&ProfileId(base + 1)].name.to_lowercase());
        assert_eq!(p.repair_profiles(), 0);
    }

    #[test]
    fn free_names_count_up() {
        let mut p = Project::empty();
        p.profiles.insert(ProfileId(1), def(1, "W 150 x 13"));
        assert_eq!(p.free_profile_name("W 150 x 13"), "W 150 x 13 2");
        assert_eq!(p.free_profile_name("HEB 200"), "HEB 200");
    }
}
```

(Check the real names of `Document::new`/`edit`/`undo` in `document.rs` and adapt the calls; the behaviour asserted stays.)

- [ ] **Step 2: Run** `cargo test -p cc_core profiles_project`. Expected: fail.
- [ ] **Step 3: Implement.** `repair_profiles` does the following, in order:
  1. truncate to `MAX_PROFILES`;
  2. set each `id` to its key;
  3. trim the name and cut it to `MAX_NAME` characters; an empty name becomes `Profile <id>`;
  4. drop profiles whose `outline()` fails;
  5. rename case-insensitive duplicates with `free_profile_name`;
  6. give a nil uid a new one (use the `Uid` nil check used elsewhere in `repair.rs`).

  Count each change. `profile_users` scans `types` for `TypeData::Column { profile: Some(id), .. }` and `TypeData::Beam { profile: Some(id), .. }`; it returns an empty Vec until Task 5 adds those fields, so put the scan in Task 5 and return `Vec::new()` here, with Task 5's test covering it.
- [ ] **Step 4: Run** the tests. Expected: pass. Also run `cargo test -p cc_core`, all of it.
- [ ] **Step 5: Commit** `git commit -s -m "Keep structural profiles as project settings"`

---

### Task 5: Types use profiles; legacy sections become profiles (format 27)

**Files:**
- Modify: `crates/cc_core/src/types.rs:377-389`. Add to `TypeData::Column` and `TypeData::Beam`:
  ```rust
  /// The structural profile of the section; when set, `section` (columns) or
  /// `width`/`height`/`shape` (beams) are its bounding box, kept in step.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  profile: Option<crate::ProfileId>,
  /// Turn of the profile in its section plane, degrees counterclockwise.
  #[serde(default, skip_serializing_if = "is_zero")]
  profile_rotation_deg: f64,
  /// The profile mirrored across its vertical axis (before turning).
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  profile_mirror: bool,
  ```
  Fix every construction site the compiler reports (`profile: None, profile_rotation_deg: 0.0, profile_mirror: false`). Run `cargo build --workspace` and follow the errors. The known sites are in `cc_core/src/project.rs:296-345, 560-580`, `cc_ifc/src/import.rs`, `cc_engine/src/commands/*`, `cc_app/src/type_dialog.rs` and the tests. Patterns that destructure with `..` need no change.
- Modify: `crates/cc_core/src/profiles_project.rs`. Add:
  ```rust
  /// The section a column or beam type draws: its profile's outline turned and
  /// mirrored, or the legacy section's; `None` for other types or a profile
  /// whose outline fails.
  pub struct TypeSection { pub outline: SectionOutline, pub profile: Option<ProfileId>, pub material: ProfileMaterial }
  impl Project { pub fn type_section(&self, data: &TypeData) -> Option<TypeSection> }
  impl ProfileShape { pub fn from_column_section(s: ColumnSection) -> ProfileShape; pub fn from_beam(width: f64, height: f64, shape: SectionShape) -> ProfileShape }
  impl Project { pub fn convert_sections_to_profiles(&mut self, translate: &dyn Fn(&str) -> String) -> usize }
  ```
  - **Mapping from legacy sections**:
    - `Rectangular` → `Rect`, `Round` → `Round`, `Pipe` → `RoundTube`;
    - `Profile{ I }` → `I` with r 0 (`d` = depth, `bf` = width, `tw` = web, `tf` = flange);
    - `Channel` → `Channel` with r 0, `Angle` → `Angle` with r 0, `Tube` → `RectTube` with ro 0, `Solid` → `Rect`.
    - Beams: `bf` = width, `d` = height.
  - **Material** comes from the type's material class: Metal → Steel, Wood → Timber, otherwise Concrete.
  - **Name** is `section_name(shape)` in mm, for example `"I 200x200x9x15"`, `"Rect 20x40"` (cm for concrete and timber rectangles, matching `cc_views::annotate::section_label`), `"Ø30"`, `"Tube 150x150x6"`. Identical (material, shape) pairs share one profile.
  - **`convert_sections_to_profiles`** is idempotent: it only touches types with `profile: None`. It returns the count of types changed.
- Modify: `crates/cc_core/src/project.rs` — at the end of `from_template` (find where `types` are inserted), call `self.convert_sections_to_profiles(n)`, so new projects start with profiles.
- Modify: `crates/cc_core/src/persist.rs`:
  - set `FORMAT_VERSION = 27`;
  - extend the version comment: "version 27 adds structural profiles (older files: one per distinct column and beam section)";
  - after `convert_opening_types`, call `project.convert_sections_to_profiles(&|k| translate(k).unwrap_or_else(|| crate::project::template_default(k).to_string()))`.
- Modify: `crates/cc_core/src/document.rs`. Add `fn sync_profiles(&mut self)`, called first in `finish` (line 789). Only when settings are touched (`self.tx.settings.is_some()`) or a Column/Beam type changed in this edit, it does the following for each type with `profile: Some(id)`:
  - compute the bbox of `type_section`;
  - write `section = ColumnSection::Rectangular{width, depth}` for columns, or `width`, `height`, `shape: Solid` for beams, through `self.type_mut(id)`, and only when the value changes (compare first so untouched types are not recorded);
  - if the profile is gone, set `profile: None`, keeping the cached box as the fallback rectangle.
- Modify: `crates/cc_core/src/repair.rs` — after `repair_profiles`, clear dangling `profile` references on types in the same way (count them), and replace non-finite `profile_rotation_deg` with 0.
- Modify: `crates/cc_core/src/profiles_project.rs` — implement `profile_users` (deferred from Task 4).
- Modify: `docs/superpowers/specs/2026-11-10-structural-profiles-design.md` — in section 1, replace "Old fields are still read but no longer written" with "Old fields stay in files as the profile's bounding box, kept in step by `sync_profiles`, so code that only needs a size keeps working; geometry, properties and IFC read the profile through `Project::type_section`."

- [ ] **Step 1: Failing tests** in `crates/cc_core/src/profiles_project.rs`:

```rust
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
    if let crate::TypeData::Column { profile, .. } = &mut copy.data { *profile = None; }
    copy.name = "Copy".into();
    let id = p.add_type(copy); // use the real add/insert helper
    let before = p.profiles.len();
    p.convert_sections_to_profiles(&|k| k.to_string());
    assert_eq!(p.profiles.len(), before);
    let same = |t: &crate::ElementType| match &t.data { crate::TypeData::Column { profile, .. } => *profile, _ => None };
    assert_eq!(same(&p.types[&id]), same(&column));
}

#[test]
fn editing_a_profile_resizes_its_types_in_one_undo_step() {
    let mut doc = Document::new(Project::from_template());
    let (tid, pid) = doc.project.types.values().find_map(|t| match &t.data {
        crate::TypeData::Beam { profile: Some(p), .. } => Some((t.id, *p)), _ => None }).unwrap();
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
```

In `crates/cc_core/src/persist.rs` tests:

```rust
#[test]
fn format_26_sections_load_as_profiles() {
    let mut p = Project::from_template();
    for t in p.types.values_mut() {
        if let crate::TypeData::Column { profile, .. } | crate::TypeData::Beam { profile, .. } = &mut t.data { *profile = None; }
    }
    p.profiles.clear();
    let text = to_ron(&p).unwrap().replacen("format_version: 27", "format_version: 26", 1);
    let back = from_ron(&text).unwrap();
    assert!(!back.profiles.is_empty());
    assert!(back.types.values().all(|t| !matches!(t.data, crate::TypeData::Column { profile: None, .. } | crate::TypeData::Beam { profile: None, .. })));
}

#[test]
fn dangling_profile_reference_is_repaired() {
    let mut p = Project::from_template();
    p.profiles.clear();
    let text = to_ron(&p).unwrap();
    let (back, repairs) = from_ron_with_repairs(&text).unwrap();
    assert!(repairs > 0);
    assert!(back.types.values().all(|t| back.type_section(&t.data).is_some() || !matches!(t.data, crate::TypeData::Column { .. } | crate::TypeData::Beam { .. })));
}
```

- [ ] **Step 2: Run** `cargo test -p cc_core`. Expected: the new tests fail, or there are compile errors to fix first.
- [ ] **Step 3: Implement** as described. In `type_section`, apply mirror (x → -x, then reverse both ring windings to keep CCW/CW) and then rotation to the outline. A legacy type without a profile resolves through `from_column_section`/`from_beam` directly.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: everything passes. If a test in another crate counted types or ids, it may need its expected numbers updated; read each failure and keep its intent.
- [ ] **Step 5: Commit** `git commit -s -m "Point column and beam types to profiles (format 27)"`

---

### Task 6: Built-in catalogs

**Files:**
- Create: `crates/cc_core/assets/profiles/{w.csv,hp.csv,ipe.csv,hea.csv,heb.csv,nbr6355_u.csv,nbr6355_ue.csv,nbr6355_z.csv,nbr6355_ze.csv,nbr6355_l.csv,nbr6355_cartola.csv,timber_sawn.csv,glulam.csv,shs.csv,rhs.csv,chs.csv}`
- Create: `crates/cc_core/src/profile_catalog.rs` (`pub mod profile_catalog;` in lib.rs)

**Interfaces:**
- Produces:
  - `Series { code: &'static str, name_key: &'static str, material: ProfileMaterial, shape: &'static str, rows: Vec<CatalogRow> }`
  - `CatalogRow { designation: String, values: Vec<(String, f64)> /* meters */, area: f64 /* m², reference only */ }`
  - `pub fn catalog() -> &'static [Series]` (parsed once with `std::sync::OnceLock`; rows that fail to parse are skipped)
  - `pub fn series(code: &str) -> Option<&'static Series>`
  - `pub fn row_to_def(series: &Series, row: &CatalogRow) -> Option<ProfileDef>` (id 0 and a new uid; the caller sets the id; `catalog: Some(CatalogRef { series, designation })`; name `"<prefix> <designation>"`, e.g. `"W 200 x 26.6"`, `"IPE 200"`, `"Ue 90x40x12x0.95"`, `"Peça 6x16"`)

**CSV format** (header row first; dimensions in millimeters; `area_cm2` from the source table):
```
designation,d,bf,tw,tf,r,area_cm2
200,200,100,5.6,8.5,12,28.5
```
- Column names are the `ProfileShape` field names for the series' shape: `shape` is `i` for W/HP/IPE/HEA/HEB, `cold_u`/`cold_lipped_u`/`cold_z`/`angle`/`cold_hat` for NBR 6355, `rect` for timber and glulam, `rect_tube` for SHS/RHS, `round_tube` for CHS.
- Fields the CSV lacks take 0 (radii, chamfer), except `ColdZ::angle` defaults to 90.
- Reference rows to include and verify by hand:
  - IPE 200: `200,200,100,5.6,8.5,12,28.5`
  - HEA 200: `200,190,200,6.5,10,18,53.8`
  - HEB 200: `200,200,200,9,15,18,78.1`
  - W 150 x 13.0: `150 x 13.0,148,100,4.3,4.9,0,16.6`
  - W 200 x 26.6: `200 x 26.6,207,133,5.8,8.4,0,34.2`
- **Coverage** (compiled by the implementer from public dimension tables; dimensions only):
  - W 150 to W 610 (the series rolled in Brazil);
  - HP 200 to HP 310;
  - IPE 80 to 600;
  - HEA and HEB 100 to 600;
  - NBR 6355:
    - U 50 to 300, Ue 75 to 300 (thicknesses 0.80 to 3.00, including the LSF sizes Ue 90x40x12, 140x40x12, 200x40x12 and U 92x38, 142x38, 202x38 tracks);
    - Z and Ze 100 to 300, L 20 to 100, cartola (hat) 20 to 50.
  - Timber (sawn) is the commercial pieces in cm: 2.5x10 to 6x16, 8x8 to 25x25.
  - Glulam is widths 9, 11.5, 14, 16, 20 cm × heights 18 to 120 cm in 4 cm lamellas, area = b × h.
  - SHS 40 to 300, RHS 50x30 to 400x200, CHS 33.4 to 323.8 (wall thicknesses as listed).
- No `area_cm2` source for a row (glulam, timber): write the exact b×h.

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_series_parses_and_has_rows() {
        let codes: Vec<&str> = catalog().iter().map(|s| s.code).collect();
        for c in ["W","HP","IPE","HEA","HEB","NBR6355-U","NBR6355-Ue","NBR6355-Z","NBR6355-Ze","NBR6355-L","NBR6355-Cartola","Timber-Sawn","Glulam","SHS","RHS","CHS"] {
            assert!(codes.contains(&c), "{c}");
            assert!(!series(c).unwrap().rows.is_empty(), "{c}");
        }
    }

    #[test]
    fn computed_areas_match_the_tables() {
        for s in catalog() {
            for row in &s.rows {
                let def = row_to_def(s, row).unwrap_or_else(|| panic!("{} {}", s.code, row.designation));
                let a = def.outline().unwrap().properties(0.0).area;
                let err = (a - row.area).abs() / row.area;
                assert!(err < 0.02, "{} {}: computed {a} table {}", s.code, row.designation, row.area);
            }
        }
    }

    #[test]
    fn reference_rows() {
        let ipe = series("IPE").unwrap();
        let r = ipe.rows.iter().find(|r| r.designation == "200").unwrap();
        let def = row_to_def(ipe, r).unwrap();
        assert_eq!(def.name, "IPE 200");
        assert_eq!(def.shape, ProfileShape::I { d: 0.2, bf: 0.1, tw: 0.0056, tf: 0.0085, r: 0.012 });
    }

    #[test]
    fn malformed_csv_rows_are_skipped() {
        let rows = parse_rows("designation,d,bf\nok,100,50\nbad,x,50\n,1\n", &["d", "bf"]);
        assert_eq!(rows.len(), 1);
    }
}
```

- [ ] **Step 2: Run** `cargo test -p cc_core profile_catalog`. Expected: fail.
- [ ] **Step 3: Implement** `parse_rows(text, fields) -> Vec<CatalogRow>`:
  - split on lines and commas, trim;
  - skip rows whose fields do not parse as finite positive numbers, and stop after 5000 rows;
  - convert mm → m and cm² → m².

  Then write the CSV data. The cold-formed rows' tabulated areas use rounded corners; if Step 4 shows them more than 2% off, set `ri` from the table (usually ri = t) instead of loosening the tolerance.
- [ ] **Step 4: Run** the tests. Expected: pass.
- [ ] **Step 5: Commit** `git commit -s -m "Add built-in steel, cold-formed, timber and tube profile catalogs"`

---

### Task 7: Columns drawn from their profile

**Files:**
- Modify: `crates/cc_geom/src/section.rs`. Add:
  ```rust
  /// The section of a column or beam type at a detail level: Fine the outline
  /// with radii, Medium without (arcs replaced by their chords' corners:
  /// rebuild the profile with r/ri/ro = 0), Coarse its bounding rectangle.
  pub fn detailed(project: &Project, data: &TypeData, detail: DetailLevel) -> Option<SectionOutline>;
  /// Solid parts of a column (outline + holes) placed in plan.
  pub fn column_body(section: &SectionOutline, position: DVec2, rotation_deg: f64) -> (Vec<DVec2>, Vec<Vec<DVec2>>);
  ```
  Keep `column_parts`/`column_loops`/`shape_rects` for now: rebar and old callers still use them. Delete whatever ends up unused at the end of Task 8.
- Modify: `crates/cc_geom/src/build.rs:492-497`. For columns, use `project.type_section(&ty.data)` and `s.add_prism_with_holes(&outer, &holes, |_| base, |_| top)` with the placed outline. Mark the faces from arcs `smooth` when the shape has radii or is round: set `Face::smooth` on side faces whose two neighbors turn by less than 30°.
- Modify: `crates/cc_geom/src/build.rs:549-554` and `column_outline` (217). `column_outline` takes `&TypeData` + `&Project`; structural column joins use the placed outer outline of `type_section`. Update its callers (`cc_views/src/annotate.rs:164`).
- Modify: the plan cut drawing of columns. Find it with `grep -rn "column_loops" crates/cc_views` and switch it to `section::detailed(project, data, view.detail)` placed by `column_body`.

**Interfaces:**
- Consumes: `Project::type_section`, `TypeSection`, `SectionOutline` (Task 5).
- Produces: `cc_geom::section::detailed`, `cc_geom::section::column_body`.

- [ ] **Step 1: Failing test** in `crates/cc_engine/tests/profiles.rs` (create the file with the usual `run` helper; copy it from `crates/cc_engine/tests/fill_patterns.rs`):

```rust
#[test]
fn a_column_with_an_ipe_profile_has_its_area_times_height() {
    let mut s = Session::default();
    // Until Task 10 exists, build the type through the core API in the test:
    let (pid, tid) = s.edit_for_test(|ctx| { /* insert IPE 200 profile; duplicate the steel column type with profile: Some(pid) */ });
    let col = run(&mut s, "column_create", json!({"type": tid, "position": [0.0, 0.0]}))["id"].as_u64().unwrap();
    let props = run(&mut s, "element_get", json!({"element": col}));
    let height = props["derived"]["height"].as_f64().unwrap();
    let volume = props["derived"]["volume"].as_f64().unwrap();
    let area = 2.0 * 0.1 * 0.0085 + (0.2 - 0.017) * 0.0056 + 4.0 * (1.0 - std::f64::consts::FRAC_PI_4) * 0.012f64.powi(2);
    assert!((volume - area * height).abs() / (area * height) < 0.003, "{volume}");
}
```

If `Session` has no test edit helper, write this test at the `cc_geom` level instead: build a `Project::from_template()`, insert the profile and type, add a column element, run `ModelGeometry::build`, and sum the volume of the column's solid with the existing volume helper. Check `cc_geom/src/build.rs` tests for how other tests do it. Once Task 10 exists, move the test to the engine using `profile_import` + `profile_type_create`.

Add a plan test: generate a floor plan cutting the column at `DetailLevel::Coarse` and `Fine`, and assert that the cut outline has 4 vertices at Coarse and more than 12 at Fine.

- [ ] **Step 2: Run** `cargo test -p cc_geom` (or `-p cc_engine profiles`). Expected: fail (the volume is the old box I's).
- [ ] **Step 3: Implement** as described.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: pass. Snapshot tests are not run by default; note any UI differences for Task 13.
- [ ] **Step 5: Commit** `git commit -s -m "Build and draw columns from their profile"`

---

### Task 8: Beams swept from general sections

**Files:**
- Modify: `crates/cc_geom/src/beam.rs`:
  - change `BeamInput::section` and `BeamGeom::section` from `Vec<(DVec2, DVec2)>` to `SectionOutline` (x across the beam (`v`), y up from the section bottom);
  - rewrite `parts()`, `add_to()` and `volume()`.
- Modify: `crates/cc_geom/src/build.rs:528-547` (`build_beams`). Get the section from `project.type_section`, shift it so y starts at 0 (`y - bbox.min.y`), and set `half` from the bbox width / 2.
- Modify: every reader of `BeamGeom::section`/`parts()`. Find them with `grep -rn "\.parts()\|section" crates/cc_geom/src/beam.rs crates/cc_geom/src/foundation* crates/cc_views crates/cc_ifc`. Strip footings build `BeamInput` too: give them a rectangle `SectionOutline` of width × thickness.

**Interfaces:**
- Produces: `BeamGeom::add_to(&self, s: &mut Solid)` (unchanged signature), `BeamGeom::volume(&self) -> f64` (unchanged), `BeamGeom::section_at_start(&self) -> Vec<Vec<DVec3>>` (start cap rings in world space; used by IFC in Task 12).

New `add_to`:
- **Mapping.** Each section vertex `(v, y)` maps to two world points: on the start face, `u = face_u(false, v)`, and on the end face, `u = face_u(true, v)`, both at `z = z(plan) + y*k`, where `k = self.stretch()` and the plan point is `self.world(u, v)`.
- **Caps.** Triangulate with `cc_geom::polygon::triangulate_with_holes(outer, holes)`. Emit start-cap triangles reversed and end-cap triangles as they are.
- **Sides.** One quad per edge of each ring (outer CCW, holes CW), joining start to end.
- **Smooth faces.** Mark side quads `smooth` where the ring turns by less than 30° at both ends of the edge.
- **Volume.** `volume()` = Σ over the cap triangles of `area_tri * (face_u(true, vc) - face_u(false, vc)) * k`, with `vc` the triangle centroid's v. This is exact because the face offsets are linear in v.

- [ ] **Step 1: Failing tests** in `cc_geom/src/beam.rs` tests (adapt the existing helper at line ~321):

```rust
#[test]
fn general_section_volume_is_area_times_length() {
    let i = cc_core::ProfileShape::I { d: 0.3, bf: 0.15, tw: 0.0071, tf: 0.0107, r: 0.0 }.outline().unwrap();
    let area = i.properties(0.0).area;
    let g = beam_with_section(i, DVec2::ZERO, DVec2::new(5.0, 0.0)); // helper: square ends, level
    assert!((g.volume() - area * 5.0).abs() < 1e-9);
    let mut s = crate::Solid::default();
    g.add_to(&mut s);
    assert!((s.volume() - area * 5.0).abs() / (area * 5.0) < 1e-6);
}

#[test]
fn round_tube_beam_is_watertight() {
    let t = cc_core::ProfileShape::RoundTube { d: 0.1, t: 0.005 }.outline().unwrap();
    let g = beam_with_section(t, DVec2::ZERO, DVec2::new(3.0, 4.0));
    let mut s = crate::Solid::default();
    g.add_to(&mut s);
    assert!(s.is_closed(), "every edge shared by two faces"); // use the existing closure check in solid.rs tests, or add one
}
```

Plus an engine test in `profiles.rs`: two beams meeting in an L with W profiles. Assert both volumes are positive and that their sum is below 2 × area × length (the mitre removed material).

- [ ] **Step 2: Run** `cargo test -p cc_geom beam`. Expected: compile failures, then failing tests.
- [ ] **Step 3: Implement**, then delete the now-unused `shape_rects` callers. Keep the legacy functions only if `rebar.rs` or `cc_views` still use them; rebar uses `section.size()`, which stays valid through the bbox cache.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: pass, including `cc_engine/tests/structure_stress.rs` and `arc_strip_footings.rs`.
- [ ] **Step 5: Commit** `git commit -s -m "Sweep beams along any profile section"`

---

### Task 9: Tags and schedules know the profile

**Files:**
- Modify: `crates/cc_views/src/annotate.rs:110-127` (`section_label`). Change its signature to `section_label(project: &Project, data: &TypeData) -> Option<String>`, returning the profile name when the type has a profile whose material is Steel or ColdFormed, else the current size label. Update the callers.
- Modify: `crates/cc_views/src/schedule.rs`. Add `ScheduleField::Profile` (text: the profile's name) and `ScheduleField::LinearMass` (kg/m, from `type_section(...).outline.properties(material density)`, where the density is `Material.physical.density`; check its exact field name in `cc_core/src/material.rs:350`). Wire them into the field lists, i18n keys `schedule-field-profile` and `schedule-field-linear-mass` in all 12 `.ftl` files, and CSV export.

- [ ] **Step 1: Failing test** in `crates/cc_engine/tests/profiles.rs`: create a beam of the template steel I type, add a schedule of beams with fields `Profile` and `LinearMass` (look at how other tests create schedules; `grep -rn "\"schedule\"" crates/cc_engine/tests`), and assert the row shows the profile's name and a mass of `area * 7850` within 1%.
- [ ] **Step 2: Run** `cargo test -p cc_engine profiles`. Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`, including the i18n tests in `cc_app`. Expected: pass.
- [ ] **Step 5: Commit** `git commit -s -m "Show profiles in tags and schedules"`

---

### Task 10: Engine commands — list, get, create, set, duplicate, delete

**Files:**
- Create: `crates/cc_engine/src/commands/profiles.rs`
- Modify: `crates/cc_engine/src/commands/mod.rs` (`pub mod profiles;`), `crates/cc_engine/src/registry.rs` (register after `fill_patterns::FillPatternExport`, line ~338)
- Modify: `crates/cc_engine/src/refs.rs` if needed. Profiles resolve by id (number) or name, as `Ref` does for fill patterns; follow `fill_patterns.rs` (`FillPatternGet`) exactly.
- Test: `crates/cc_engine/tests/profiles.rs`

**Interfaces:**
- Consumes: `ProfileDef`, `ProfileShape::{default_for, set_param, params, CODES}`, `ParamSketch`, `Project::{profile_named, profile_users, free_profile_name}`, `EditCtx::profiles_mut`, `cc_engine::commands::sketch::CurveArg`, `cc_core::profile::{from_curves, written}`.
- Produces commands. Each is a struct with doc comments, `Deserialize + JsonSchema`, `impl Command`:
  - `profiles_list { material: Option<String>, shape: Option<String> }` → `[{id, name, material, shape, area, linear_mass_steel, users}]`, where `linear_mass_steel` is the area × 7850.
  - `profile_get { profile: Ref }` → `{id, name, material, shape, params: {name: value}, sketch: {params, outline, openings}?, outline: {outer: [[x,y]], holes}, properties: SectionProperties, catalog?, used_by: [type names]}`.
  - `profile_create { name: Option<String>, material: String, shape: String, params: Option<Map<String,f64>>, curves: Option<Vec<CurveArg>>, openings: Option<Vec<Vec<CurveArg>>>, sketch_params: Option<Vec<{name, value}>> }` → `{id, name}`. With `curves`, the shape is `sketch`: the curves are turned into `ProfilePoint`s with literal numbers through `from_curves` + `written`, and `sketch_params` are declared.
  - `profile_set { profile: Ref, name: Option<String>, material: Option<String>, params: Option<Map<String,f64>>, sketch: Option<Value> /* merge patch over ParamSketch serde form */ }`. A failing outline → `EngineError::invalid(<ProfileError>)`, with no undo step.
  - `profile_duplicate { profile: Ref, name: Option<String> }` → `{id, name}`.
  - `profile_delete { profile: Ref, replace_with: Option<Ref> }`. If the profile is in use and there is no `replace_with`, return an error listing the type names. With a replacement, the types are repointed in the same edit (`EditCtx::type_mut`).

Every edit goes through `super::edit(session, "<undo name>", |ctx| ...)`, the same helper `fill_patterns.rs` uses. Names are checked: not empty, at most `MAX_NAME`, unique ignoring case.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn create_get_set_delete() {
    let mut s = Session::default();
    let out = run(&mut s, "profile_create", json!({"material": "steel", "shape": "i",
        "name": "PS 300", "params": {"d": 0.3, "bf": 0.15, "tw": 0.008, "tf": 0.0125, "r": 0.0}}));
    let id = out["id"].as_u64().unwrap();
    let got = run(&mut s, "profile_get", json!({"profile": "ps 300"}));
    assert_eq!(got["id"].as_u64(), Some(id));
    let area = got["properties"]["area"].as_f64().unwrap();
    assert!((area - (2.0 * 0.15 * 0.0125 + 0.275 * 0.008)).abs() < 1e-9);

    let err = run_err(&mut s, "profile_set", json!({"profile": id, "params": {"tw": 0.5}}));
    assert_eq!(err.code, "invalid");
    run(&mut s, "profile_set", json!({"profile": id, "params": {"tw": 0.01}}));
    assert_eq!(run(&mut s, "profile_get", json!({"profile": id}))["params"]["tw"], 0.01);

    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "profile_get", json!({"profile": id}))["params"]["tw"], 0.008);
    run(&mut s, "profile_delete", json!({"profile": id}));
    assert!(run(&mut s, "profiles_list", json!({})).as_array().unwrap().iter().all(|p| p["id"].as_u64() != Some(id)));
}

#[test]
fn sketch_profile_from_curves() {
    let mut s = Session::default();
    let out = run(&mut s, "profile_create", json!({"material": "steel", "name": "Box",
        "curves": [{"rectangle": [[-0.1, -0.15], [0.1, 0.15]]}],
        "openings": [[{"rectangle": [[-0.09, -0.14], [0.09, 0.14]]}]]}));
    let got = run(&mut s, "profile_get", json!({"profile": out["id"]}));
    assert_eq!(got["shape"], "sketch");
    assert_eq!(got["outline"]["holes"].as_array().unwrap().len(), 1);
}

#[test]
fn deleting_a_used_profile_needs_a_replacement() {
    let mut s = Session::default();
    let used = run(&mut s, "profiles_list", json!({})).as_array().unwrap().iter()
        .find(|p| p["users"].as_u64().unwrap() > 0).unwrap().clone();
    let other = run(&mut s, "profile_create", json!({"material": "concrete", "shape": "rect", "name": "R"}))["id"].clone();
    let err = run_err(&mut s, "profile_delete", json!({"profile": used["id"]}));
    assert!(err.message.contains("replace_with"), "{}", err.message);
    run(&mut s, "profile_delete", json!({"profile": used["id"], "replace_with": other}));
    let r = run(&mut s, "profile_get", json!({"profile": "R"}));
    assert!(r["used_by"].as_array().unwrap().len() >= 1);
}
```

(`run_err` is the error variant of `run`; copy it from another test file, e.g. `grep -rn "fn run_err" crates/cc_engine/tests`. Use whatever undo command name the catalog has: `cargo run -p cc_cli -- commands | grep -i undo`.)

- [ ] **Step 2: Run** `cargo test -p cc_engine --test profiles`. Expected: fail.
- [ ] **Step 3: Implement** the commands.
- [ ] **Step 4: Run** `cargo test -p cc_engine`. The `reference` test will fail until docs are regenerated: run `cargo run -p cc_cli -- reference > docs/commands.md`, then `cargo test --workspace`. Expected: pass.
- [ ] **Step 5: Commit** `git commit -s -m "Add profile commands"`

---

### Task 11: Catalog import, profile types, type_set, element_get, analytical export

**Files:**
- Modify: `crates/cc_engine/src/commands/profiles.rs`, `crates/cc_engine/src/registry.rs`
- Modify: `crates/cc_engine/src/commands/types.rs` (`type_set`: accept `"profile": "<name or id>"`, `"profile_rotation_deg"`, `"profile_mirror"` in `params`; resolve the name to `ProfileId` before the merge patch; `type_get` shows `profile` as its name)
- Modify: `crates/cc_engine/src/element_params.rs` or wherever `element_get` builds `derived` (`grep -rn "\"derived\"" crates/cc_engine/src`): add `section: {profile, area, linear_mass, ix, iy, wx, wy}` for columns and beams.
- Modify: `crates/cc_engine/src/commands/analytical.rs:97` (`analytical_export`): each member gets `section: {profile, area, ix, iy, j}`.
- Modify: `crates/cc_engine/src/commands/framing.rs:42` (`post_type`): prefer a column type whose profile material is Steel/ColdFormed (keep the old fallback).

**Interfaces:**
- Produces commands:
  - `profile_catalog { series: Option<String>, search: Option<String> }` → without `series`: `[{code, name, material, shape, count}]`; with it: `{code, rows: [{designation, name, params}]}` (search: case-insensitive substring of the designation).
  - `profile_import { series: String, designations: Vec<String>, on_clash: Option<String> /* "keep" (default) | "rename" */ }` → `{imported: [{id, name}], kept: [name], unknown: [designation]}`. One undo step. `keep` reuses an existing profile with the same name.
  - `profile_type_create { profile: Ref, category: String /* "column" | "beam" */, name: Option<String>, material: Option<String> }` → `{id, name}`. The material defaults to the first project material whose `MaterialClass` equals `profile.material.material_class()`, else the template's concrete. The type is cloned from the first type of the category, then given the profile; `sync_profiles` fills the box. The type name defaults to the profile name, made unique.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn import_from_catalog_and_make_types() {
    let mut s = Session::default();
    let series = run(&mut s, "profile_catalog", json!({}));
    assert!(series.as_array().unwrap().iter().any(|x| x["code"] == "IPE"));
    let rows = run(&mut s, "profile_catalog", json!({"series": "IPE", "search": "20"}));
    assert!(rows["rows"].as_array().unwrap().iter().any(|r| r["designation"] == "200"));

    let out = run(&mut s, "profile_import", json!({"series": "IPE", "designations": ["200", "270", "999"]}));
    assert_eq!(out["imported"].as_array().unwrap().len(), 2);
    assert_eq!(out["unknown"][0], "999");
    let again = run(&mut s, "profile_import", json!({"series": "IPE", "designations": ["200"]}));
    assert_eq!(again["kept"][0], "IPE 200");

    let t = run(&mut s, "profile_type_create", json!({"profile": "IPE 200", "category": "beam"}));
    let ty = run(&mut s, "type_get", json!({"type": t["id"]}));
    assert_eq!(ty["params"]["profile"], "IPE 200");
    assert!((ty["params"]["height"].as_f64().unwrap() - 0.2).abs() < 1e-9, "bbox cached");

    let beam = run(&mut s, "beam_create", json!({"type": t["id"], "start": [0.0, 0.0], "end": [6.0, 0.0]}))["id"].clone();
    let e = run(&mut s, "element_get", json!({"element": beam}));
    assert!((e["derived"]["section"]["area"].as_f64().unwrap() - 0.00285).abs() / 0.00285 < 0.02);

    run(&mut s, "profile_import", json!({"series": "HEB", "designations": ["200"]}));
    run(&mut s, "type_set", json!({"type": t["id"], "params": {"profile": "HEB 200"}}));
    let ty = run(&mut s, "type_get", json!({"type": t["id"]}));
    assert!((ty["params"]["width"].as_f64().unwrap() - 0.2).abs() < 1e-9);

    let a = run(&mut s, "analytical_export", json!({}));
    let text = a.to_string();
    assert!(text.contains("HEB 200"), "members carry their section");
}
```

(Check `analytical_export`'s real parameters and output shape; if it writes to a path, use its in-memory form or a temporary file in the test, as other tests do.)

- [ ] **Step 2: Run** `cargo test -p cc_engine --test profiles`. Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Regenerate** `docs/commands.md` (`cargo run -p cc_cli -- reference > docs/commands.md`), then run `cargo test --workspace`. Expected: pass.
- [ ] **Step 5: Docs.** Add a "Structural profiles" section to `docs/automation.md` and `docs/automacao.md` (Portuguese) with a 6-line example: `profile_import` IPE 200, `profile_type_create`, `beam_create`, `profile_set`, `profile_get`. Commit: `git commit -s -m "Import catalog profiles and make types from them"`

---

### Task 12: IFC export and import of profiles

**Files:**
- Modify: `crates/cc_ifc/src/export.rs`:
  - replace `section_profile(ColumnSection)` (655-690) with `profile_def(&mut self, def: &ProfileDef, outline: &SectionOutline) -> Id`. It writes the name in the `ProfileName` attribute (2nd argument, currently `Null`) and the parametric `Ifc*ProfileDef` for:
    - `I` → `IFCISHAPEPROFILEDEF` (args: type, name, position, OverallWidth = bf, OverallDepth = d, WebThickness = tw, FlangeThickness = tf, FilletRadius = r or `$`, FlangeEdgeRadius `$`, FlangeSlope `$`);
    - `Channel` → `IFCUSHAPEPROFILEDEF` (Depth = d, FlangeWidth = bf, tw, tf, r);
    - `T` → `IFCTSHAPEPROFILEDEF`;
    - `Angle` → `IFCLSHAPEPROFILEDEF` (Depth = h, Width = b, Thickness = t, r);
    - `ColdLippedU` → `IFCCSHAPEPROFILEDEF` (Depth = d, Width = bf, WallThickness = t, Girth = lip, InternalFilletRadius = ri);
    - `ColdZ` with angle 90 → `IFCZSHAPEPROFILEDEF`;
    - `RectTube` → `IFCRECTANGLEHOLLOWPROFILEDEF` (with InnerFilletRadius/OuterFilletRadius from ro);
    - `RoundTube` → `IFCCIRCLEHOLLOWPROFILEDEF`;
    - `Rect` with no chamfer → `IFCRECTANGLEPROFILEDEF`;
    - `Round` → `IFCCIRCLEPROFILEDEF`;
    - everything else → `IFCARBITRARYCLOSEDPROFILEDEF` (with an `IFCPOLYLINE` of `IFCCARTESIANPOINT`s), or `IFCARBITRARYPROFILEDEFWITHVOIDS` when there are holes.

    Apply the type's rotation and mirror through the profile's `IFCAXIS2PLACEMENT2D` (rotation as `RefDirection`). For mirror, use the arbitrary form with the mirrored outline.
  - Columns (875-900): use the type's profile.
  - Beams (~908-958): extruded when square-ended and level (as now). Otherwise the faceted B-rep from the new `add_to` solid comes for free.
  - **Material profile sets.** Per column and beam type, write:
    - `IFCMATERIALPROFILE(name, $, material, profileDef, $, $)`;
    - `IFCMATERIALPROFILESET(name, $, (profile), $)`;
    - `IFCRELASSOCIATESMATERIAL` from the type to the set;
    - per element, `IFCMATERIALPROFILESETUSAGE(set, $, $)` associated through the existing material association. Find where element materials are associated today (`grep -n "IFCRELASSOCIATESMATERIAL" crates/cc_ifc/src/export.rs`) and route columns and beams to the usage instead of the plain material.
  - Update `section_area` (2633) to use `type_section(...).outline.properties(0).area`.
- Modify: `crates/cc_ifc/src/import.rs` (~1080-1100 and wherever `ColumnSection` is built):
  - read the parametric profile defs into `ProfileShape`s, `IfcArbitraryClosedProfileDef`/`WithVoids` into a `ParamSketch` with literal points, and the `ProfileName`;
  - create a profile per (name, shape), reusing one with the same name and equal shape; on a different shape, use `free_profile_name`;
  - set the type's `profile`. The material comes from `IfcMaterialProfile.Material` when present.
- Modify: `crates/cc_ifc/tests/round_trip.rs`. Add `profiles_round_trip`: a project with one column and one beam type per shape code (16, the sketch with a hole), exported then imported. Assert each type's `profile` resolves to an equal `ProfileShape` (parametric kinds) or the same area within 0.5% (arbitrary kinds), and that names survive.
- Create: `crates/cc_ifc/tests/fixtures/profiles.ccad`. Write it from the test's project with `cc_core::to_ron` in a one-off `#[ignore]` test or a small example, then commit the file.
- Modify: `.github/workflows/ci.yml` (add `crates/cc_ifc/tests/fixtures/profiles.ccad` to the IFC validation job's list) and `AGENTS.md` (the CI fixture sentence).

- [ ] **Step 1: Write** `profiles_round_trip` (failing).
- [ ] **Step 2: Run** `cargo test -p cc_ifc profiles_round_trip`. Expected: fail.
- [ ] **Step 3: Implement** export and import.
- [ ] **Step 4: Run** `cargo test --workspace`. Then, if Python is available, validate the fixture locally: `pip install -r scripts/requirements-ifc.txt && cargo run -p cc_ifc --example export_ifc -- crates/cc_ifc/tests/fixtures/profiles.ccad out.ifc && python scripts/validate_ifc.py out.ifc`. Expected: no schema errors. If Python is unavailable, say so in the PR and rely on CI.
- [ ] **Step 5: Commit** `git commit -s -m "Write and read profiles and material profile sets in IFC"`

---

### Task 13: Profile Manager dialog

**Files:**
- Create: `crates/cc_app/src/profiles_dialog.rs` (`mod profiles_dialog;` in `main.rs`/`lib.rs`, wherever `fill_patterns_dialog` is declared)
- Create: `assets/icons/ui/profiles.svg` (24 grid, white strokes, `stroke-width` 1.75, round caps and joins, an I-section outline with a soft fill at 20% opacity)
- Modify: `crates/cc_app/src/commands.rs`. Add `UiCommand::Profiles`: icon `profiles`, label key `cmd-profiles`, description key `cmd-profiles-desc`, no shortcut, engine command `profiles_list`, always available. It opens the dialog (`AppState::profiles_dialog: Option<profiles_dialog::State>`); add it to the five places listed in the exploration (icons ~503, labels ~709, descriptions ~910, engine names ~1171, dispatch ~1862).
- Modify: `crates/cc_app/src/ribbon.rs:663-684` (`structure_tab`): add a *Profiles* group with `UiCommand::Profiles` first. Also add it to the Manage tab next to *Fill Patterns* (`grep -n "FillPatterns" crates/cc_app/src/ribbon.rs`).
- Modify: `assets/i18n/*.ftl` (12 files). The keys:
  - `cmd-profiles`, `cmd-profiles-desc`, `profiles-title`, `profiles-search`;
  - `profiles-new`, `profiles-duplicate`, `profiles-delete`, `profiles-import`, `profiles-replace-with`;
  - `profiles-params`, `profiles-properties`, `profiles-used-by`, `profiles-edit-sketch`;
  - `profile-material-steel`, `profile-material-cold-formed`, `profile-material-timber`, `profile-material-concrete`;
  - `profile-shape-<code>` for all 16 codes;
  - `profile-prop-area`, `profile-prop-mass`, `profile-prop-ix`, `profile-prop-iy`, `profile-prop-wx`, `profile-prop-wy`, `profile-prop-rx`, `profile-prop-ry`, `profile-prop-j`;
  - `profiles-catalog-title`, `profiles-catalog-series`, `profiles-catalog-filter`, `profiles-catalog-add`;
  - `profile-series-<code>` for the 16 series.

  pt-BR is written first; the others are proper translations.

**Behaviour:**
- **Shell.** `dialog::Dialog` with title `profiles-title`, OK first and Cancel last. The whole session is wrapped in `dialog::session`, so Cancel undoes every command run inside it.
- **Left column.**
  - Search box, then a collapsible `table::section` per `ProfileMaterial` listing the names from `profiles_list`.
  - Buttons *New* (a menu of shape codes → `profile_create` with defaults and a free name), *Duplicate*, *Delete* and *Import from catalog*.
  - *Delete* asks for a replacement with a combo box when `users > 0`, then runs `profile_delete`.
- **Center.** A preview painted with egui's painter: the outline from `profile_get` scaled to fit with 10% margin, filled with the theme's soft tint, holes cut (paint triangles from `cc_geom::polygon::triangulate_with_holes`), centroid cross, x/y axes dashed, and overall dimensions d and bf as text. Colors come from `theme::palette(ctx)`.
- **Right.**
  - Name, material combo, then the parameter `table::row`s: `DragValue` in millimeters, writing meters. A change runs `profile_set` merged under a key (`Session::run_merged`, key `"profile-param"`) so one drag is one undo step. An error shows in red under the table and the value reverts.
  - Then the read-only properties, in cm² / kg/m / cm⁴ / cm³ / cm.
  - For sketch profiles, an *Edit sketch* button (Task 14).
- **Catalog sub-dialog.** Series combo, filter, multi-select list of designations (checkboxes), and *Add*, which runs `profile_import`.

**Tests:**
- [ ] **Step 1: Failing UI test** in `crates/cc_app/tests/ui_tests.rs` (follow the existing patterns there to open a dialog by its ribbon button label):

```rust
#[test]
fn profile_manager_imports_and_cancels() {
    let mut harness = app_harness(); // existing helper
    harness.get_by_label("Profiles").click(); // pt-BR? use the label the harness language uses; see other tests
    harness.run();
    harness.get_by_label("Import from catalog").click();
    harness.run();
    // choose series IPE, tick "200", click Add
    // ...use the existing helpers for combo boxes and checkboxes in ui_tests.rs
    harness.get_by_label("OK").click();
    harness.run();
    assert!(harness.state().session.project().profile_named("IPE 200").is_some());
    // reopen, delete it with a replacement, Cancel: still there
}
```

Fill the elided steps with the exact helper calls the file already uses for combo boxes and checkboxes. If the harness cannot drive a combo box, set the dialog state's `series` field directly through `harness.state_mut()`, as other tests do.

- [ ] **Step 2: Run** `cargo test -p cc_app profile_manager`. Expected: fail.
- [ ] **Step 3: Implement** the dialog, the icon, the ribbon entries and the i18n keys.
- [ ] **Step 4: Run** `cargo test -p cc_app` (the i18n key tests included), then `cargo test --workspace`. Run the app to look at it: `cargo run -p cc_app -- examples/casa_simples.ccad`, then open *Structure → Profiles*. Then rewrite the UI snapshots that changed because of the new ribbon button (`UPDATE_SNAPSHOTS=1 cargo test -p cc_app snapshots`) and inspect the diffs.
- [ ] **Step 5: Commit** `git commit -s -m "Add the Profile Manager"`

---

### Task 14: Profile picker in Type Properties and sketch editing

**Files:**
- Modify: `crates/cc_app/src/type_dialog.rs`. The current section fields use `ColumnSection`/`SectionShape` 12 times. For Column and Beam types, replace them with:
  - a searchable combo of the project's profiles grouped by material (each entry with its name), with *Manage…* opening the Profile Manager;
  - `profile_rotation_deg` (an angle `DragValue`) and `profile_mirror` (a checkbox);
  - a read-only row with the profile's d × bf.

  Changes go through `engine_edit::element_type` (the `type_set` path).
- Modify: `crates/cc_app/src/sketch_mode.rs`:
  - add `Target::Profile(ProfileId)`. The sketch opens with the profile's sketch curves (`profile::to_curves` of its resolved points), reference planes at x = 0 and y = 0, and no level;
  - *Finish* runs `profile_set` with `sketch: {outline: <written points>, openings: [...]}`, keeping the expressions of points that did not move. Reuse what `component_sketch` does for pieces (`grep -n "fn keep_expressions\|expressions" crates/cc_app/src/component_sketch.rs`);
  - *Cancel* returns to the Profile Manager.
- Modify: `crates/cc_app/src/tools.rs:?` (3 uses of `ColumnSection`): column and beam previews use the type's `type_section` outline.

- [ ] **Step 1: Failing UI test**: open Type Properties of the template beam, pick "IPE 200" (imported first through `Session::run(ProfileImport{..})`), press OK, and assert that `type_get` shows `profile: "IPE 200"`. A second test opens a sketch profile's *Edit sketch*, presses Finish without changes, and asserts the profile is unchanged (equal `ParamSketch`).
- [ ] **Step 2: Run** `cargo test -p cc_app`. Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`. Run the app and check: import IPE 200, make a beam type, draw a beam, look at it in 3D and in a section at Coarse and Fine.
- [ ] **Step 5: Commit** `git commit -s -m "Pick profiles in Type Properties and sketch free profiles"`

---

### Task 15: Documentation and final checks

**Files:**
- Modify: `AGENTS.md`. Add an architecture bullet **Structural profiles** describing:
  - `cc_core::structural_profile` (`ProfileDef`, `ProfileShape`, `ParamSketch`, outline rules, limits);
  - `section_props`;
  - `profile_catalog` (CSV assets);
  - `Project::profiles` as settings and `type_section`;
  - the bounding-box cache kept by `sync_profiles`;
  - format 27;
  - `cc_geom::section::detailed` and the beam sweep;
  - IFC parametric profiles + `IfcMaterialProfileSet`;
  - `cc_engine::commands::profiles`;
  - `profiles_dialog.rs`, the type picker and `sketch_mode::Target::Profile`.

  Also update the sentence on `TypeData::Column/Beam` and section shapes in the cc_geom bullet if it mentions them.
- Modify: `docs/interface.md` and `docs/interface.pt-BR.md`. One paragraph on the Profile Manager under the Structure tab.

- [ ] **Step 1: Run the full check list**

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p cc_cli -- reference > docs/commands.md && git diff --exit-code docs/commands.md
cargo run -p cc_core --example casa_simples && git diff --stat examples/
```

Expected: everything is clean. If `casa_simples.ccad` changed (format 27 and template profiles), commit it.
- [ ] **Step 2: Commit** `git commit -s -m "Document structural profiles"`
- [ ] **Step 3: Push the branch and open a PR.** Only when the user asks.
