# Structure tab by material (sub-project 2a)

Part of the Structure tab redesign (concrete, steel, timber). Sub-project 1,
the parametric profile library, is merged (#313). This sub-project, 2a,
splits the Structure tab by material and adds a multi-storey grid frame.
Sub-project 2b, the parametric steel building (portal frames, purlins and
girts), follows with its own spec and uses a host element that owns real
members.

## Goals

- A material selector (Concrete, Steel, Timber) on the Structure tab
  that swaps the material's own groups. The shared groups always show.
- Column and beam tools started from the Structure tab use types of the
  selected material. A default type is created when the project has none.
- A multi-storey grid frame: columns on every storey and beams on every
  level of a range, with optional secondary beams, in one undo step.

Out of scope:

- Portal frames, purlins and girts (2b).
- The truss redesign (3), LSF and wood framing (4), connections (5).
- A structural flag on floors.

## Material of a type

`cc_core`:

- `StructuralMaterial { Concrete, Steel, Timber }` with `ALL`, `code()`
  (`concrete`, `steel`, `timber`), `from_code` and an i18n `key()`.
- `Project::structural_material(&TypeData) -> Option<StructuralMaterial>`
  for column and beam types (`None` for other data):
  - With a profile, it comes from the profile's `ProfileMaterial`:
    - Steel and ColdFormed give Steel;
    - Timber gives Timber;
    - Concrete gives Concrete.
  - Without a profile (a legacy section that makes no outline), it comes
    from the type's material class, as `profile_material_for` already does:
    - Metal gives Steel;
    - Wood gives Timber;
    - anything else gives Concrete.
- `Project::types_of_material(category, material)` gives the types of a
  category whose structural material is `material`, in id order.

## Engine: `structural_type_for`

`structural_type_for { category: "column" | "beam", material }` is an
edit command. It returns `{ id, name, created }`.

- When the category has a type of that material, it returns the first
  one (`created: false`, no undo step).
- Otherwise it creates one in a single undo step, through the same code
  as `profile_import` and `profile_type_create`:
  - Steel: catalog `W` row `W200x26,6`.
  - Timber: catalog `Glulam` row `MLC 9x32`.
  - Concrete: a concrete `rect` profile 0.20 x 0.40 named `20x40`.
    An existing profile of that name and shape is reused.
- The type is named after the profile. The material is the first project
  material of the profile's class (`type_material`).
- An unknown category or material is `InvalidArguments`.

## Engine: `grid_frame_create`

An edit command with these parameters:

- `grids`: ids; default every grid.
- `base_level` and `top_level`: `Ref`s; default the lowest level and the
  one above it.
- `column_type` and `beam_type`: `Ref`s. When missing, they come from
  `material` through `structural_type_for`.
- `material`: default `steel`.
- `secondary: { parallel_to: grid id, spacing: meters }`, optional.

What it creates:

- The levels from `base_level` up to `top_level` are ordered by
  elevation. Each consecutive pair is a storey.
- On every storey:
  - A column at each grid intersection on the lower level, with its top
    attached to the upper level. Columns that already exist are skipped,
    as `ColumnsAtGrids` does.
  - Main beams on the upper level along each grid, between neighboring
    columns of the storey below. Spans already modeled are skipped, as
    `BeamsOnGrids` does.
  - The beams belong to the upper level, with their tops at its
    elevation (`top_offset` 0).
- Secondary beams, when asked for, on every upper level:
  - A bay is the quadrilateral between two consecutive grids parallel to
    `parallel_to` (within 1°) and two consecutive crossing grids. All
    four of its corners must be grid intersections.
  - In each bay, `n = ceil(width / spacing) - 1` beams run parallel to
    `parallel_to`, evenly spaced. `width` is the distance between the
    two parallel grids.
  - They end on the crossing grids' beams.
  - Bays that are not such quadrilaterals are skipped and counted.
- The whole run is one undo step (`commands::edit`). It reuses the span
  and intersection helpers of `commands::structure`, which become
  `pub(crate)` where needed.

Limits and errors:

- At most `MAX_MEMBERS` (2000) members per run. The count is checked
  before any edit, and a larger run is refused with `InvalidArguments`.
- `base_level` must be lower than `top_level`.
- Fewer than two grids, or no intersection, is `InvalidArguments`.
- `spacing` must be finite and at least 0.3 m.
- `parallel_to` must be one of the grids.

The command returns `{ columns, beams, secondary, skipped, skipped_bays }`.
`columns`, `beams` and `secondary` are lists of ids; `skipped` and
`skipped_bays` are counts.

## App

State:

- `AppState::structure_material: StructuralMaterial`, saved in
  `Preferences` (eframe storage key `structure_material`, validated on
  load, default Concrete).

Ribbon (`ribbon::structure_tab`):

- The first group, *Material*, holds three stacked toggle buttons
  (`selectable_value`) that report their role and state through
  `widget_info`.
- Groups by material:
  - **Concrete:** *Structure* with Column, Beam and Floor
    (`Place(Floor)`), and *Reinforcement*.
  - **Steel:** *Structure* with Column, Beam and BracesX; *Grid* with
    ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns and GridFrame;
    and Truss.
  - **Timber:** *Structure* with Column and Beam, and *Grid* (the same
    four commands).
- Shared groups, always shown: Profiles, Foundation, Analytical,
  Documentation, Datum and Selection.

Tools:

- `Tool::Column` and `Tool::Beam` gain `material: Option<StructuralMaterial>`.
- `UiCommand::StructurePlace(Category, StructuralMaterial)` is what the
  Structure tab's Column and Beam buttons are. Running it:
  1. runs `structural_type_for`, so a missing type is created as an
     undo step;
  2. starts the tool on that type, with the filter set.
- `tools::tool_types` keeps only the types of the filter's material.
  Tools started elsewhere keep `material: None` and list every type.
- ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns and BracesX, when run
  from the Structure tab, pass the `type_ref` of the selected material,
  resolved through `structural_type_for`.

Grid frame dialog:

- `UiCommand::GridFrame`, icon `grid-frame`, opens the dialog in
  `grid_frame_ui.rs`, built on `dialog::Dialog` with OK and Cancel.
- Fields:
  - base level and top level;
  - column type and beam type, listed by `types_of_material`;
  - *Secondary beams*: a checkbox, the grid picker and the spacing.
- OK runs `grid_frame_create`. Errors go to the dialog.
- It is available when the project has at least two grids. Otherwise it
  is disabled with the reason.

All interface text is new i18n keys in the 12 languages.

## Testing

Engine (`crates/cc_engine/tests/structure_materials.rs`, declared in
`tests/main.rs`):

- `structural_material`:
  - steel, cold-formed, timber and concrete profiles;
  - a legacy section without an outline, through the material class.
- `structural_type_for`:
  - returns an existing type with no undo step;
  - creates the steel, timber and concrete defaults;
  - a second call reuses them;
  - bad arguments are refused.
- `grid_frame_create` on 3 x 3 grids and 3 storeys:
  - column and beam counts;
  - column tops attached to the level above;
  - beams at the upper levels;
  - a second run creates nothing and reports the skips;
  - secondary beams per bay, with the count from the spacing;
  - a skewed bay is skipped;
  - over `MAX_MEMBERS` it is refused with no undo step;
  - the run is one undo step.

App (`ui_tests.rs`):

- The selector swaps the groups and the choice is kept.
- Column in Steel starts on a W type, and the options bar lists only
  steel types.
- The grid frame dialog creates the members, and Cancel leaves the
  model unchanged.

Also:

- `docs/commands.md` regenerated.
- Docs updated: `docs/interface.md`, `docs/interface.pt-BR.md`,
  AGENTS.md and the CHANGELOG.
