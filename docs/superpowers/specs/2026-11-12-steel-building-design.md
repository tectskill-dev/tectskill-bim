# Parametric steel building (sub-project 2b)

Part of the Structure tab redesign. 2a (the Structure tab by material and
the grid frame) comes before it. 2b adds a parametric steel shed: portal
frames, purlins and girts, generated as real columns and beams that a host
element keeps in step with its parameters.

The user approved approach A (a host element owning real members) and
asked me to make the remaining design decisions alone. Those decisions are
recorded here.

## Goals

- A `SteelBuilding` element that holds the parameters:
  - placement: level, origin, rotation;
  - span, eave height, roof slope, roof shape (gable or mono-pitch);
  - number of bays and bay spacing;
  - column and rafter types;
  - optional purlins and girts, each with a type and a spacing.
- Its members are ordinary `Column` and `Beam` elements. They show up in
  schedules, IFC, the analytical model and tags with no special case.
- Changing a parameter regenerates the members in the same transaction.
  A member keeps its id while its role exists, so tags, dimensions and
  rebar stay attached.
- Editing or deleting a member by hand detaches it:
  - The host stops managing that role.
  - Later regenerations leave the edited member alone, and do not recreate
    a deleted one.

Out of scope:
- Haunches and tapered members.
- Bracing, which the grid tools and `braces_between_columns` already do.
- End-wall columns and girts.
- Base plates and connections (sub-project 5).

## Model (`cc_core::steel_building`)

`ElementKind::SteelBuilding(SteelBuilding)`:

- Category `SteelBuilding`.
- `TypeData::SteelBuilding {}`: the type is created with the first
  building, the same way Shaft does it.

The struct's fields:

| field | meaning | default | bounds |
|---|---|---|---|
| `origin: DVec2` | plan point of the first frame's left column | | finite |
| `rotation_deg: f64` | turn of the local x axis (across the span) | 0 | finite |
| `span: f64` | column axis to column axis, m | 20 | 2..=100 |
| `eave_height: f64` | column top above the level, m | 6 | 1..=50 |
| `slope_percent: f64` | roof slope | 10 | 0..=100 |
| `roof: RoofShape` | `Gable` (ridge at mid-span) or `Mono` (rising toward x = span) | Gable | |
| `bays: u32` | frames = bays + 1 | 5 | 1..=100 |
| `bay_spacing: f64` | frame to frame, m | 6 | 1..=30 |
| `column_type`, `rafter_type: ElementTypeId` | | | must be column and beam types |
| `purlins: Option<Secondary>` | `Secondary { type_id, spacing }`, spacing along the rafter's plan run | Some, 1.5 | spacing 0.3..=10 |
| `girts: Option<Secondary>` | on both long sides, spacing up the wall | Some, 1.5 | same |
| `members: Vec<FrameMember>` | `FrameMember { role: MemberRole, id: ElementId }` | empty | |
| `detached: Vec<MemberRole>` | roles the user took over | empty | |

`MemberRole` is a small `Copy + Ord` serde struct:
`{ kind: RoleKind, frame: u16, index: u16 }`, where `RoleKind` is
`Column`, `Rafter`, `Purlin` or `Girt`.

### Layout

`SteelBuilding::layout(&self, project) -> Vec<(MemberRole, ElementTypeId, ElementKind)>`
is deterministic. Positions are in local coordinates, x across the span
and y along the building, then mapped to plan through origin and rotation.
Frame `k` is at `y = k * bay_spacing`.

- **Columns:** at `(0, y_k)` (index 0) and `(span, y_k)` (index 1).
  Base at the level, unconnected `height = eave_height`, structural.
- **Rafters:**
  - Gable: two per frame. Index 0 goes from `(0, y_k)` to
    `(span / 2, y_k)`; index 1 from `(span, y_k)` to `(span / 2, y_k)`.
  - Mono: one per frame, from `(0, y_k)` to `(span, y_k)`.
  - The location line is the top of the rafter. It starts at
    `top_offset = eave_height + rafter depth`, where the depth is the
    height of the rafter type's section box. It rises at
    `slope_percent / 100` over the plan run (`end_offset`).
  - The ends inside the columns stop at the column faces through the
    existing beam joins.
- **Purlins:** one beam per bay along y, from frame `k` to frame `k + 1`.
  - Their x positions run along each roof plane:
    - Gable: on each half, from the eave to the ridge, the first at
      `x = column half depth`, then every `spacing`, with one at the
      ridge (`span / 2 - purlin half width`, mirrored for the other half).
    - Mono: from 0 to span.
  - The top sits at the rafter's top elevation at that x, plus the purlin
    depth, so the purlin bears on the rafter.
  - `frame` is the bay and `index` is the purlin line.
- **Girts:** one beam per bay along y, on each long side.
  - Their axis is at `x = -(column half depth + girt half width)`
    (side 0) and `x = span + (column half depth + girt half width)`
    (side 1).
  - Heights run from `spacing` above the level to below the eave, every
    `spacing`.
  - `index` holds `side * 1000 + row`.
- At most `MAX_MEMBERS = 4000` members. `layout` stops adding beyond that.

Section depths and widths come from `Project::type_section` boxes. A
missing section reads as 0.2 m.

### Sync (`EditCtx::finish`, first step)

`sync_steel_buildings` runs first, before the other syncs. It sees which
elements the edit itself changed:

1. Members changed or deleted in this transaction whose host did not
   change: their roles move to the host's `detached` list, and the
   `members` entry is dropped. The element stays as the user left it.
2. Hosts changed or added in this transaction, every role of the layout
   not in `detached`:
   - An existing member is rewritten in place, keeping its id: kind,
     type and level.
   - Otherwise a new element is added and recorded.
   - Members whose role is no longer in the layout are deleted.
3. A host added in this transaction whose `members` ids already belong to
   another host (a copy) has its `members` cleared before step 2, so it
   gets its own.

Other rules:

- Deleting a host deletes its members (`EditCtx::remove_element`).
- Deleting a member just leaves a detached role.
- Undo and redo restore everything through the transaction, as usual.
- Repair:
  - drops `members` entries whose element is missing or is not a column
    or beam, and repeated ids or roles;
  - drops `detached` repeats;
  - resets out-of-bound parameters to their defaults;
  - resets a type that is missing or of the wrong category to the
    category's default type, and drops the whole building when there is
    none.

### Drawing

- **Plans:** draw the building's footprint, from local `(0, 0)` to
  `(span, bays * bay_spacing)` turned into plan, as a thin dashed
  rectangle owned by the host. This lets the host be picked and selected
  in plans. The name tag `SteelBuilding` + mark is not drawn.
- **3D views and sections:** nothing. The members are the geometry.
- **IFC:** export leaves the host out (like masses), and its members
  export as usual.

## Engine

`steel_building_create`:

- Parameters:
  - `level`;
  - `origin`, default `[0, 0]`;
  - `rotation`;
  - `span`, `eave_height`, `slope_percent`;
  - `roof` (`gable` / `mono`);
  - `bays`, `bay_spacing`;
  - `column_type`, `rafter_type`;
  - `purlins` and `girts`: `{ type, spacing }` or `null` for none,
    default on.
- Default types:
  - Columns and rafters use the catalog `W` row `W310x38,7`.
  - Purlins and girts use the catalog `NBR6355-Ue` row
    `Ue 150x60x20x2,00`.
  - Each is imported and typed when missing, like `structural_type_for`
    does, through a shared helper `catalog_type(s, category, series,
    designation)` in `structure_material.rs`.
- One undo step. Returns `{ id, members }`.
- Out-of-range values are refused, not clamped.

`element_get` shows the building's parameters in `params`. `element_set`
merges a patch over them. The repair check refuses out-of-range values,
and the sync regenerates the members in the same step. `element_get`'s
`derived` lists the member ids by role kind.

## App

- **Steel panel:** a *Steel Building* button (`UiCommand::SteelBuilding`,
  icon `steel-building`) opens a dialog (`steel_building_ui.rs`,
  `dialog::Dialog`) with:
  - span, eave height and slope (%);
  - roof shape;
  - bays and bay spacing;
  - origin x and y, and rotation;
  - column and rafter types (steel types);
  - purlins and girts, each a checkbox, a type (steel types, cold-formed
    included) and a spacing.

  OK runs `steel_building_create` on the active level.
- **Properties palette:** for a selected building, rows for every
  parameter, edited through `engine_edit::element`, plus a *Reset
  members* button that clears `detached` and regenerates.
- **i18n:** all text in the 12 languages.

## Testing

- **cc_core:** layout counts for gable and mono, with and without purlins
  and girts:
  - columns `2(b + 1)`;
  - rafters `2(b + 1)` for gable, `b + 1` for mono;
  - purlins and girts per bay as computed.

  Rafter slope and ends, and the bounds (`MAX_MEMBERS`), are also checked.
- **Sync:**
  - Creating a host adds its members.
  - Changing the span moves them and keeps their ids.
  - Fewer bays deletes the extra members; more bays adds new ones.
  - Moving a member detaches it, and a later span change leaves it alone.
  - Deleting a member is not undone by regeneration.
  - Deleting the host removes its members.
  - A copy gets its own members.
  - Undo restores the members.
  - Repair drops dangling members.
- **Engine:**
  - `steel_building_create` with defaults makes the catalog types once.
  - `element_set` regenerates in one undo step.
  - Out-of-range values are refused.
  - IFC export has the columns and beams and no host.
- **App:** the dialog creates a building, and editing the span in the
  palette regenerates it.

## Decisions made while planning

- The roof enum is `FrameRoof { Gable, Mono }`, because `cc_core::RoofShape`
  already exists.
- Columns rise to the rafters' top at the column axis
  (`eave_height + rafter depth`). They are turned so their depth runs
  across the span, so the existing beam joins stop the rafters at the
  column faces. The eave height stays the rafters' underside.
- What detaches a member:
  - An edit to its kind, type or level detaches it. Marks, comments,
    phases and paints do not.
  - A deleted member is always detached.
  - An edited member is detached only when its building did not change in
    the same edit.
- Changing a type the building uses regenerates the building.
- `sync_steel_buildings` runs right after `sync_profiles`, before every
  sync that touches elements.
- A building added in an edit drops member entries that belong to another
  building or no longer exist. Copies therefore get their own members.
- Purlin lines start at the column face. A regular line closer than one
  purlin width to the ridge is skipped.
- A girt row's height is its top. Rows run at `spacing`, `2·spacing` and
  so on, strictly below the eave.
- `element_set` refuses to change `members`. `detached` stays editable,
  and *Reset Members* clears it.
- `ElementKind::SteelBuilding` holds a `Box`. There is no format bump,
  thanks to serde defaults.
