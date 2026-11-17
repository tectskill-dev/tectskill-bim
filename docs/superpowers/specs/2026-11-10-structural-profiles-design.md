# Structural profiles — design

Status: approved in conversation, 2026-11-10. Sub-project 1 of the Structure redesign.

## Context and roadmap

The Structure tab is being redesigned around three materials: concrete, steel and timber. The work is split into five sub-projects. Each one gets its own spec, plan and PR:

1. **Parametric profile library** (this document): a profile manager, steel, cold-formed, timber and concrete profiles, used by column and beam types.
2. **New Structure tab**: Concrete / Steel / Timber panels, with tools and types filtered by material. Heavy steel frame (rolled and welded sections) lives in the Steel panel.
3. **Parametric truss element**: chords, posts and diagonals with profiles, steel and timber typologies, editable after creation.
4. **Light framing systems**: light steel framing (LSF, cold-formed) and wood frame. Studs, tracks, headers, blocking and bracing, generated from walls and floors.
5. **Parametric steel and timber connections**: plates, bolts, welds, angles and timber connectors at beam-column, beam-beam and column-base joints.

Today a column type holds a `ColumnSection` (rectangular, round, profile, pipe) and a beam type holds a width, a height and a `SectionShape` (solid, I, channel, angle, tube). There is no profile catalog, no shared or named profile, no cold-formed shape, no root radius, and a beam cannot be round. Section properties are not computed. IFC export writes `Ifc*ProfileDef`s but no `IfcMaterialProfileSet`.

## Goals

- Profiles are first-class project objects, named and shared by types, edited in one place.
- Standard parametric shapes cover rolled, welded, cold-formed, timber and concrete sections. A free parametric sketch covers everything else.
- Built-in catalogs: W/HP, IPE/HEA/HEB, NBR 6355 cold-formed, commercial sawn timber and glulam, and tubes.
- Section properties come from the geometry.
- Every operation is an engine command. The app's Profile Manager calls them.

Non-goals: code checking (NBR 8800, 14762, 7190), loads, connections, trusses, framing systems, and the new tab layout. Each of these belongs to a later sub-project.

## 1. Data model — `cc_core::structural_profile`

```rust
pub struct ProfileId(pub u32);

pub struct ProfileDef {
    pub uid: Uid,
    pub name: String,                // unique; e.g. "W 200 x 26.6", "Ue 90x40x12x0.95"
    pub material: ProfileMaterial,   // Steel, ColdFormed, Timber, Concrete
    pub shape: ProfileShape,
    pub catalog: Option<CatalogRef>, // series + designation it came from
    pub description: String,
}

pub enum ProfileShape {
    I { d, bf, tw, tf, r },
    IAsym { d, bf_top, tf_top, bf_bot, tf_bot, tw },
    Channel { d, bf, tw, tf, r },
    T { d, bf, tw, tf, r },
    Angle { h, b, t, r },
    ColdU { d, bf, t, ri },
    ColdLippedU { d, bf, lip, t, ri },
    ColdZ { d, bf, lip, t, ri, angle },
    ColdHat { d, bf, lip, t, ri },
    RectTube { h, b, t, ro },
    RoundTube { d, t },
    Rect { b, h, chamfer },
    Round { d },
    ConcreteT { h, bf, hf, bw },
    ConcreteL { h, bf, hf, bw },
    Sketch(ParamSketch),
}

pub struct ParamSketch {
    pub params: Vec<ComponentParam>,       // Length / Angle / Number kinds only
    pub outline: Vec<ProfilePoint>,        // u, v, radius as cc_core::expr expressions
    pub openings: Vec<Vec<ProfilePoint>>,
}
```

All lengths are in meters. Field names follow the usual section notation:
- `d` is the depth, `bf` the flange width, `tw` and `tf` the web and flange thicknesses;
- `r` is the root radius, `ri` the inner bend radius and `ro` the outer corner radius;
- `t` is a plate or wall thickness and `lip` the length of a lip.

- `Project::profiles: BTreeMap<ProfileId, ProfileDef>` is tracked as settings (`EditCtx::profiles_mut`) and has a serde default.
- `ProfileDef::outline() -> Result<SectionOutline, ProfileError>` returns the outline and its openings in meters, centered on the centroid. Arcs are chorded at `ARC_STEP`. Errors are values: a non-closed or crossing outline, thicknesses that do not fit, or a bad expression. The outline is bounded by `MAX_POINTS`, `MAX_OPENINGS` and `MIN_THICKNESS`.
- `ProfileDef::properties(density)` returns `SectionProperties`: area, linear mass, centroid, Ix, Iy, Wx, Wy, rx, ry, approximate J and painted perimeter. They are computed by polygon (Green's theorem) integration over the outline and openings. Tests check them against closed-form formulas.
- Types: `TypeData::Column` and `TypeData::Beam` gain `profile: Option<ProfileId>`, `profile_rotation_deg` and `profile_mirror` (serde defaults). When `profile` is set, it replaces `section`/`shape`.
- Migration (format 27): each distinct `ColumnSection`/`SectionShape` + size becomes one profile (deduplicated, named after the type's section), and the types point to it. Old fields stay in files as the profile's bounding box, kept in step by `sync_profiles`, so code that only needs a size keeps working; geometry, properties and IFC read the profile through `Project::type_section`.
- Repair (`repair_profiles`):
  - drops profiles whose outline fails;
  - fixes duplicate names and ids;
  - resets dangling type references to a rectangle of the type's old box size and reports the count.
- `Project::profile_users(id)` lists the types using a profile. Deletion requires a `replace_with` when the profile is in use.

## 2. Catalogs, geometry, IFC

### Catalogs — `cc_core::profile_catalog`

The catalogs are built-in CSV data in `assets/profiles/*.csv`, loaded with `include_str!` and parsed once (bounded, errors as values). The series are:
- `W`, `HP`
- `IPE`, `HEA`, `HEB`
- `NBR6355-U`, `-Ue`, `-Z`, `-Ze`, `-L`, `-Cartola`
- `Timber-Sawn`, `Glulam`
- `SHS`, `RHS`, `CHS`

Each row is a designation plus the shape's dimensions taken from public dimension tables; nothing else is copied. A test checks that each computed area is within 2% of the table area, which is stored in the CSV for this check only. Catalogs are not stored in project files: `profile_import` copies the chosen rows in, recording their `CatalogRef`.

### Geometry — `cc_geom::section`

`column_parts`/`column_loops` and the beam builder take the profile outline instead of `ColumnSection`/`SectionShape`. The section's rotation and mirror apply. Beam joins in `beam.rs` use the convex hull of the placed section.

Detail levels:
- **Coarse**: bounding rectangle in plans and sections.
- **Medium**: the outline without radii.
- **Fine**: the outline with radii.

Section cuts hatch in the type's material. Curved facets are marked `Face::smooth`.

### IFC — `cc_ifc`

- **Export**: each standard shape is written as its `Ifc*ProfileDef`:
  - I → `IShape`, Channel → `UShape`, T → `TShape`, Angle → `LShape`;
  - ColdLippedU → `CShape`;
  - RectTube → `RectangleHollow`, RoundTube → `CircleHollow`, Rect → `Rectangle`, Round → `Circle`;
  - the cold shapes without an IFC equivalent (including ColdZ, as `IfcZShapeProfileDef` has no lips), asymmetric I, concrete T/L and sketches → `IfcArbitraryClosedProfileDef`/`IfcArbitraryProfileDefWithVoids`.

  Each column and beam type gets an `IfcMaterialProfileSet` naming the profile.
- **Import**: reads those profile definitions and `IfcMaterialProfileSet`s, creates profiles by name (reusing one with the same name and shape) and points the types to them.
- `tests/round_trip.rs` covers every shape.
- `analytical_export` adds each member's section properties.

## 3. Engine commands — `cc_engine::commands::profiles`

| Command | Kind | Purpose |
|---|---|---|
| `profiles_list` | query | Project profiles, filtered by material and shape, with area, mass and user count |
| `profile_get` | query | Shape, parameters, resolved outline and properties |
| `profile_create` | edit | From `shape` + values, or from sketch `curves` (`CurveArg`) + `params` |
| `profile_set` | edit | Merge patch over values and name; a failing outline refuses the edit |
| `profile_duplicate` | edit | Copy under a new name |
| `profile_delete` | edit | `replace_with` required when in use; one undo step |
| `profile_catalog` | query | Built-in series and designations, with search |
| `profile_import` | edit | Designations of a series into the project; `on_clash`: keep or rename |
| `profile_type_create` | edit | Column or beam type with a profile and the material matching it |

`type_set` accepts `"profile": "<name>"`, and `element_get` shows the section properties. The tests live in `crates/cc_engine/tests/profiles.rs`. `docs/commands.md` is regenerated, and `docs/automation.md`/`automacao.md` get a section.

## 4. App — Profile Manager (`profiles_dialog.rs`)

The manager opens from *Structure → Profiles* and *Manage → Profiles*. It is a `dialog::Dialog` with OK/Cancel inside `dialog::session`.

- **Left**: the project's profiles grouped by material, with search and *New*, *Duplicate*, *Delete* (with a replacement picker) and *Import from catalog*.
- **Center**: a to-scale preview, dimensioned with the parameter names, showing centroid and axes.
- **Right**: the parameter table (`table.rs`) and live section properties.
- **Free sketch**: *Edit sketch* opens a sketch session (`sketch_mode`, `Target::Section`) with reference planes. Dimensions become parameters and points keep expressions, as in the component environment.
- **Catalog import**: series selector, a filterable list, multi-select.
- **Type Properties** of columns and beams: a searchable profile picker replaces the section fields.

UI conventions:
- i18n keys in all 12 languages;
- own SVG icons;
- kittest UI tests: open, import, edit a parameter, cancel.

## Testing

- `cc_core`: properties against closed forms (rectangle, circle, tube, I with r = 0), every shape's outline validity, sketch expressions, migration of every old `ColumnSection`/`SectionShape`, repair of hostile files.
- `cc_core` catalogs: every row parses and matches its table area.
- `cc_engine`: each command, undo, delete with replacement, import clash handling.
- `cc_ifc`: round trip of every shape. The CI IFC validation adds a `profiles.ccad` fixture.
- `cc_app`: UI tests as above.
