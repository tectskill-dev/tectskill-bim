# Parametric trusses for steel and timber (sub-project 3)

Part of the Structure tab redesign. It builds on:

- 2a, the Structure tab by material;
- 2b, the parametric steel building. 2b introduced a host element whose
  members are real columns and beams, kept in step in `EditCtx::finish`.

This sub-project redesigns trusses on that same mechanism, so that a truss
stays parametric and fits both steel and timber.

The user asked for this ("redesenhar treliças para atender melhor aço e
madeira", redesign the trusses to serve steel and timber better) and told
me to make the design decisions alone.

## Goals

- **The member-host mechanism becomes shared.**
  - The steel building's member sync, removal, copy handling and repair
    are generalized to any host kind through a `MemberHost` abstraction in
    `cc_core::member_host`.
  - It is a trait implemented by the host data.
  - `Element::member_host()` and `member_host_mut()` return
    `Option<&dyn MemberHost>` (and `&mut dyn`), plus a dispatch by
    `ElementKind`.
  - `SteelBuilding` is ported to it with no change in behavior, and the
    2b tests still pass.
- **A `Truss` host element**, category `Truss` with `TypeData::Truss {}`.
  Its parameters:
  - `start`, `end`: the plan span line, between the truss's support points;
  - `base_offset`: the bottom chord axis above the level at the supports;
  - `shape`: `Flat`, `Mono` or `Gable`;
  - `height`: chord to chord for `Flat`, or the rise at the apex or high
    end for `Mono` and `Gable`; for those two the bottom chord is level;
  - `pattern`: `Pratt`, `Howe`, `Warren`, `KingPost`, `QueenPost` or
    `Fink`. King post, queen post and Fink apply only to `Gable`. A
    pattern that does not apply falls back to `Pratt`, a decision made
    in `check`.
  - `panels`: the number of bottom-chord panels. The default comes from
    the span (about 1.5 m each). The minimum is 2, the maximum 64. A
    `Gable` needs an even count.
  - `top_chord_type`, `bottom_chord_type` and `web_type`: beam types;
  - `post_type`: a column type, for the vertical web members;
  - `copies`: how many trusses (1 to 200);
  - `spacing`: distance between copies, to the left of start→end,
    0.3 to 20 m;
  - `members` and `detached`: as in 2b.
- **Members.**
  - Chords are beams, one per panel and per chord. The top chord follows
    the shape.
  - Diagonals are sloped beams, using the beam's `end_offset`.
  - Verticals are columns of `post_type`, from the bottom chord axis to
    the top chord axis. A vertical shorter than 0.05 m is skipped.
  - Each copy is offset along the left normal by `k * spacing`.
  - At most `MAX_MEMBERS = 4000`; a truss over it is refused (see the amendment below).
- **Material defaults** (the material comes from the Structure tab's
  selector, or the command's `material`):

  | material | chords | web | posts |
  |---|---|---|---|
  | steel | `Ue 150x60x20x2,00` | `Ue 100x50x17x2,00` | column type of `Ue 100x50x17x2,00` |
  | timber | `Peça 6x16` | `Peça 6x12` | column type of `Peça 6x12` |
  | concrete | not offered; the command refuses it | | |

  The catalog series are `NBR6355-Ue` and `Timber-Sawn`. Types are made
  through `catalog_type`, which 2b added.

## Geometry

- The local x axis runs along start→end, with `L = |end − start|`. Panel
  points on the bottom chord are at `x_i = i·L/panels`.
- The top chord height above the bottom chord at x:
  - `Flat`: `h`;
  - `Mono`: `h·x/L`;
  - `Gable`: `h·(1 − |2x/L − 1|)`.

  The top-chord panel points are at the same x values. For `Mono` and
  `Gable` the top chord meets the bottom chord at the supports where its
  height is 0. The vertical there is skipped, being under 0.05 m.
- Webs per pattern, at panel `i` between `x_i` and `x_{i+1}`:
  - **Pratt:** verticals at every inner panel point; diagonals slope
    down toward the middle.
  - **Howe:** verticals at every inner panel point; diagonals slope
    up toward the middle.
  - **Warren:** no verticals; diagonals alternate.
  - **KingPost** (Gable): one vertical at mid-span and two diagonals from
    the mid-span base to the quarter points of the top chords. With
    `panels` = 4 these are the canonical members; other counts use the
    panel points nearest the quarters.
  - **QueenPost:** verticals at the third points and a diagonal from each
    of them to mid-span.
  - **Fink** (W): from the bottom chord's third points, diagonals up to
    the top chord's quarter points and to the apex. No verticals.
- Member roles: `{ kind: TopChord | BottomChord | Diagonal | Vertical,
  copy: u16, index: u16 }`.

## Engine

- `truss_create` keeps its name and parameters for compatibility, and
  also accepts:
  - `shape`;
  - patterns `kingpost`, `queenpost` and `fink`;
  - `top_chord_type`, `bottom_chord_type` and `post_type`
    (`chord_type` sets both chords);
  - `copies`, `spacing` and `material`.

  It now creates a `Truss` host instead of a group and returns
  `{ id, members }`.

  Breaking change: the old group of loose members is gone. A test that
  read the group is updated.
- `element_get` and `element_set` show and patch the truss parameters
  like the building's. `members` cannot be changed.
- The analytical model, schedules and IFC need nothing new: members
  export as usual, and the host is skipped like the steel building.

## Views

Plans draw the truss host as its span line (start to end), dashed, for
each copy. That is enough to pick and select it.

## App

- The Truss tool in the Structure tab moves to both the steel and timber
  groups. It keeps its two-click placement, and the options bar now shows:
  - shape and pattern;
  - height and panels;
  - copies and spacing.
- The tool runs `truss_create` with the material of the tab.
- The properties palette shows rows for the truss parameters and a
  *Reset Members* button, sharing code with the building's palette.
- New text is in 12 languages.

## Testing

- **member_host:** the steel building tests pass unchanged. A generic test
  covers sync on a truss: create, change panels, detach a member, delete
  the host, copy, undo.
- **Truss layout**, per pattern and shape:
  - member counts (for example, a Pratt flat truss with n panels has
    2n chords, n+1 verticals (see the amendment below) and n diagonals);
  - the gable apex height;
  - the King-post member set with 4 panels;
  - an odd panel count on a gable is refused;
  - copies are offset by the spacing.
- **Engine:**
  - `truss_create` defaults for steel and timber, and concrete refused;
  - `element_set` of `panels` regenerates in one undo step.
- **App:** the Truss tool with options creates a truss, and the palette
  edits it.

## Decisions made while planning

- **Roles.** Roles cross the `MemberHost` trait as packed `RoleKey(u64)`
  values that sort like the typed roles. Each host keeps its own typed
  role and its file format.
- **Bounds.**
  - Height: 0.1–20 m, default 1 m.
  - Span: 0.5–200 m.
  - Default spacing: 1.5 m.
  - Default panels: round(L/1.5), clamped to 2–64 and made even on a
    gable.
- **Verticals.**
  - Pratt and Howe get verticals at the inner panel points.
  - A mono truss also gets a post at its high end.
- **End verticals on flat trusses (amended after the final review).** A
  flat truss gets posts at both supports (panel points 0 and n) for every
  pattern, Warren with an odd panel count included, so each support node
  is triangulated; without them the default flat Pratt's supports were
  reached by the bottom chord alone. A flat Pratt of n panels has n + 1
  verticals. Mono and gable trusses need none where the chords meet.
  `Truss::check` refuses a truss whose full layout (all copies) would
  exceed `MAX_MEMBERS`, so a layout is never cut short, and bounds
  `base_offset` to ±10 km.
- **Skipped diagonals.** A diagonal whose two ends differ in height by
  less than 0.05 m is skipped.
- **Gable-only patterns.** King post, queen post and Fink need a gable
  with at least 2, 4 and 6 panels. Otherwise they fall back to Pratt.
- **Member kinds.**
  - Chords and diagonals are beams with `BeamVertical::Center`.
  - Verticals are columns turned along the span.
- **Mirroring.** A mirror moves the span line to the mirrored last copy.
- **`truss_create` arguments.**
  - `height` is still required.
  - `bottom_offset` sets `base_offset`.
  - `material` defaults to steel; concrete is refused.
  - `chord_type` sets both chords. The top and bottom chord types
    override it.
- **Members follow their host's phase and option.** As in 2b after its
  final review, members follow their host's phase and design option, and
  a new or pasted host clears `members` and makes its own. Task 1's port
  must keep this behavior, which commit 20839ef added after the plan was
  written.
