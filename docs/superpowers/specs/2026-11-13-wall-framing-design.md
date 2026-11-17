# Light steel frame and wood frame walls (sub-project 4)

Part of the Structure tab redesign; the user asked for "permitir LSF e SF"
("allow LSF and SF"). The heavy steel frame (SF) is covered by 2a (grid
frames) and 2b (steel buildings). This sub-project frames walls: light
steel framing (LSF) with cold-formed studs and tracks, and wood framing
with sawn timber studs and plates.

It uses the shared member-host mechanism (`cc_core::member_host`) from
sub-project 3. The user told me to make the design decisions alone.

## Goals

- A `WallFraming` host element (category `WallFraming`,
  `TypeData::WallFraming {}`) attached to one straight wall. It holds:
  - `wall: ElementId`: the host wall;
  - `system`: `LightSteel` or `Wood`;
  - `stud_type`: a column type;
  - `track_type`: a beam type, for the bottom and top tracks or plates;
  - `header_type`: a beam type, for headers and sills at openings;
  - `spacing`: stud spacing, on center, from the wall start. 0.2–1.2 m,
    default 0.4 for LSF and 0.6 for wood;
  - `double_top`: an extra top plate. Wood only; default on for wood,
    off for LSF;
  - `blocking`: a row of horizontal blocking at mid-height between studs.
    Default off;
  - `members` and `detached`, as in the other hosts.
- The members are real columns (studs, king studs, jack studs and
  cripples) and real beams (tracks or plates, headers, sills, blocking),
  laid out in the wall's frame.
- The framing regenerates whenever its own parameters, its wall, or any
  door or window hosted by that wall changes. `MemberHost` gains
  `fn inputs(&self) -> Vec<ElementId>`, the other elements whose change
  regenerates the host. The default is empty. `WallFraming` returns the
  wall plus the wall's hosted openings, gathered by the host's
  `layout`/`inputs` from the project.
- Deleting the wall deletes its framing and the members. The framing is
  hosted the way rebar is.
- One framing per wall. Creating a second one on the same wall is refused.

Out of scope:
- curved walls (refused);
- sheathing and cladding layers;
- bracing straps;
- connections (sub-project 5);
- framing floors and roofs.

## Layout

The wall frame is the body centerline (`Project::wall_axis`), with u
along the wall from 0 to length L. The base is `wall_base_z` and the top
is `wall_top_z`. Let `t` be the track or plate depth (the height of the
track type's section box) and `n` the number of top plates (1, or 2 with
`double_top`).

- **Bottom track or plate.** One beam along the whole wall, its top at
  base + t. Not split at doors, which keeps the model simple; real practice
  cuts the plate at doors, and that is left for later.
- **Top tracks or plates.** `n` beams along the whole wall, stacked under
  the wall top.
- **Studs.** Columns at u = 0, at u = L, and at every `k·spacing` in
  between.
  - A stud that falls inside an opening, widened by the stud width on
    each side, is dropped.
  - A stud closer than one stud width to the stud before it is dropped.
  - Studs run from base + t to top − n·t.
  - They are turned so that their section's depth runs across the wall:
    rotation = wall direction + 90°.
- **Openings.** For each door or window hosted by the wall, at center
  u = offset, with width w and height h from the wall base plus the sill
  (0 for doors):
  - **King studs:** full height, at `u0 − s` and `u1 + s`, where
    `u0 = offset − w/2`, `u1 = offset + w/2` and `s` is half the stud
    width.
  - **Jack studs:** from the bottom plate to the underside of the header,
    beside the king studs, one stud width further in.
  - **Header:** a beam from `u0` to `u1`, its bottom at the opening top.
  - **Sill:** windows only, a beam from `u0` to `u1`, its top at the sill
    height.
  - **Cripples:** studs at the regular spacing positions inside
    `(u0, u1)`. Above the header they run up to the top plate; below a
    window's sill they run down to the bottom plate.
  - Openings that overlap each other or the wall ends are clipped to the
    wall.
- **Blocking.** When `blocking` is on, beams at mid-height between each
  pair of neighboring full-height studs.
- **Limit.** At most `MAX_MEMBERS = 4000`.
- **Roles.**
  - Kinds: `BottomTrack`, `TopTrack`, `Stud`, `KingStud`, `JackStud`,
    `Header`, `Sill`, `Cripple` and `Blocking`.
  - Fields: `index`, the stud position or plate number, and `opening`,
    the order of the opening along the wall.
  - Roles are packed as `RoleKey`.
  - Keys stay stable when an unrelated opening moves.

## Defaults

| system | studs (column type of) | tracks/plates | headers/sills |
|---|---|---|---|
| LightSteel | `Ue 90x40x12x0,95` (NBR6355-Ue) | `U 92x38x0,95` (NBR6355-U) | `Ue 140x40x12x0,95` |
| Wood | `Peça 6x8` (Timber-Sawn) | `Peça 6x8` | `Peça 6x16` |

All of them are made through `catalog_type`.

## Engine

`wall_framing_create` takes:
- `walls`: ids;
- `system`: `light_steel`, the default, or `wood`;
- `spacing`;
- `stud_type`, `track_type` and `header_type`;
- `double_top`;
- `blocking`.

It creates one framing per wall in one undo step. It refuses curved walls
and walls that already have framing, before any edit, and returns
`{ framings: [{ id, wall, members }] }`.

`element_get` and `element_set` handle the parameters as for the other
hosts. `wall` cannot be changed.

## Views

Plans draw nothing for the host itself. The studs show as columns in the
plan cut. To select the host, the members' properties show a *Framing*
row whose button selects it. In elevations and sections the members are
drawn as usual.

## App

- The **Light Steel Frame** button goes in the Steel material panel and
  the **Wood Frame** button in the Timber panel. Each opens a small
  dialog (system preset, spacing, types, double top plate, blocking).
- OK frames the selected walls, or all walls of the active level when
  none are selected.
- The properties palette for a framing gets parameter rows and *Reset
  Members*.
- New text is in 12 languages.

## Testing

- **`member_host` inputs:**
  - moving the wall regenerates its framing, keeping the stud ids;
  - adding a window adds king studs, jack studs, a header, a sill and
    cripples, and drops the studs inside the opening;
  - deleting the wall removes everything;
  - undo works.
- **Layout:**
  - a 3.6 m wall with LSF at 0.4 m has 10 studs;
  - a 1.2 m wide window at the center gives 2 king studs, 2 jack studs,
    1 header, 1 sill and the cripples above and below it;
  - a door has no sill;
  - double top plates for wood;
  - blocking count;
  - curved walls are refused.
- **Engine:**
  - default types for both systems;
  - a second framing on a wall is refused;
  - `element_set` of `spacing` regenerates the members in one undo step.
- **App:** the dialog frames a selected wall.

## Decisions made while planning

These amend the sections above where they differ.

- **Host inputs.**
  - `inputs` takes the project.
  - A framing regenerates when the edit touches an input, an input's
    type, or a door or window the wall hosted before the edit.
  - `member_level` puts the members on the wall's level.
- **Openings.**
  - Jack studs sit at the opening edges.
  - King studs sit one stud width outside them.
  - The header lies over the jacks. The sill runs between them.
- **Stud rotation.** Studs turn with the wall direction, because a
  column's depth is its local y. Task 3 checks this on the real Ue
  profile.
- **Ends and indexes.**
  - End studs are flush with the wall ends and have fixed indexes.
  - A regular stud too close to an end stud is dropped.
  - Index rules:
    - cripples are 2k above and 2k+1 below;
    - kings and jacks are 0 on the start side and 1 on the end side;
    - top plates count from the topmost.
  - Blocking uses the track type, sits at mid-height and is keyed by
    its left stud.
- **Framing beams.**
  - They don't join, so they keep their lengths.
  - Pieces under 0.05 m are skipped.
  - The header stays under the top tracks.
  - A sill is made only if it clears the bottom track.
- **Double top.** `double_top` is ignored for light steel.
- **Curved walls.** A wall bent after framing keeps an empty framing.
  The members come back when it is straightened.
- **Opening order.** Openings are ordered by offset, then id.
- **Copies.**
  - Copying a wall copies its framing.
  - A framing copied alone is skipped.
- **Repair.** Repair drops framings whose wall is gone, and extra
  framings on the same wall.
- **App.**
  - OK frames the straight, unframed walls among those selected, or among
    the active level's walls when none is selected.
  - The palette shows the system read-only.
  - Both buttons share one icon.
- **Known gaps.**
  - A level elevation change that moves a wall's top without editing the
    wall does not regenerate its framing.
  - A wall top attached to a roof is ignored.
