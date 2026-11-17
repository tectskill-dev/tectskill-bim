# Steel and timber connections (sub-project 5)

This is the last part of the Structure tab redesign. The user asked for
"junções de aço e madeira, tudo paramétrico" (steel and timber joints,
all parametric) and told me to make the design decisions alone.

## Goals

- **The element.** A `Connection` element (category `Connection`,
  `TypeData::Connection {}`) sits at one end of a structural member and
  draws the plates and bolts of a standard joint.
- **Parametric geometry.**
  - The geometry is derived in `cc_geom` from the member's current
    geometry on every build, so it follows the member when the member is
    edited.
  - It is not a member host: plates and bolts are not columns or beams.
    They are this element's own solids.
- **Kinds:**

  | kind | material | at | parts |
  |---|---|---|---|
  | `BasePlate` | steel | a column's base | plate under the column, anchor bolts in a rectangular pattern |
  | `EndPlate` | steel | a beam end that stops at a column face | plate welded to the beam end, bolts through the plate |
  | `FinPlate` | steel | a beam end meeting a column or beam | a vertical plate along the beam web, bolts through it |
  | `TimberBolts` | timber | a beam end meeting a column | a bolt group through both members, plus washers as short cylinders |
  | `Hanger` | steel, for timber | a beam end meeting a beam | U-shaped bent plate (base and two sides) under the beam end |

- **Where it sits.** `Connection { member: ElementId, end: MemberEnd
  (Base | Start | End), kind, params }`, plus `params` per kind:
  - plate thickness, width and length (0 means "from the member section
    plus `edge`");
  - bolt diameter;
  - bolt rows and columns;
  - bolt spacing and edge distance;
  - anchor length (base plates).
- **Bounds.** Every length is 1 mm to 2 m. Rows and columns are 1 to 10.
  Out-of-range values are refused by the commands and reset by repair.
- **Lifecycle.** The connection is removed with its member, as hosted
  elements are. A member end carries at most one connection; repair drops
  any further ones. Copying a member does not copy its connections.

## Geometry (`cc_geom::connection`)

- The member's frame comes from its geometry:
  - columns: position, rotation and base z;
  - beams: `BeamGeom` end points, direction and the end's resolved face
    (`BeamEnd` and its extent), plus the section box from
    `Project::type_section`.
- **BasePlate:**
  - A box centered under the column, its top at the column base.
  - Size: section box + 2·edge, or the given size. Thickness t.
  - Anchor bolts are cylinders, of diameter d, through the plate,
    extending `anchor_length` below it. They are laid out in
    rows × columns at the edge distance.
- **EndPlate:**
  - A box at the beam end face, normal to the beam axis.
  - Size: section box + 2·edge. Thickness t, outside the beam end face.
  - Bolts are cylinders normal to the plate, length = t + 20 mm.
- **FinPlate:**
  - A vertical box in the beam's web plane: length 2·edge + (columns−1)·spacing
    along the beam, height = 0.8·section height, thickness t.
  - It starts at the beam end face.
  - Bolts run across the web.
- **TimberBolts:**
  - Bolt cylinders across the beam width at the end, in rows × columns,
    spaced at the spacing, starting `edge` from the end face.
  - Bolt length = beam width + 2·washer thickness.
- **Hanger:**
  - A base plate under the beam end and two side plates up the beam
    sides to 0.6 of its height, each of thickness t.
  - It extends `edge` along the beam.
- **Output.** Solids go into `ModelGeometry` as category `Connection`
  with material steel. `ModelGeometry::connections` keeps each
  connection's plate volume, bolt count and bolt length.
- **Failure.** A connection whose member is missing, or whose member end
  does not match the kind, builds nothing and records a warning in
  `ModelGeometry`. It is not an error.

## Engine

- **`connections_create`.** Arguments:
  - `kind`;
  - `members`: ids, defaulting to every member of the active level's
    kind (columns for `BasePlate`, beams otherwise);
  - `params`: partial; anything missing takes the kind's defaults.

  Behavior:
  - It places one connection at each member end that suits the kind:
    - `BasePlate`: every column;
    - `EndPlate` and `TimberBolts`: beam ends whose `BeamEnd` is
      `Column`;
    - `FinPlate`: ends at columns or beams;
    - `Hanger`: ends at beams.
  - It skips ends that already have a connection.
  - It all happens in one undo step, and returns `{ created, skipped }`.
- **`element_get` and `element_set`.**
  - They show and patch `params`, refusing bad values.
  - `derived` gives the plate volume, plate mass (7850 kg/m³), bolt count
    and bolt diameter.
- **Schedules.** A connection schedule has the fields `ConnectionKind`,
  `PlateMass`, `BoltCount` and `BoltDiameter`, plus the existing count,
  mark and comments. The new fields show values only on connections.

## IFC

- Each connection exports as an `IfcElementAssembly` (`USERDEFINED`, its
  kind in the `ObjectType`). It aggregates:
  - an `IfcPlate` per plate;
  - an `IfcMechanicalFastener` per bolt (`ANCHORBOLT` for base plates,
    `BOLT` otherwise).
- Bodies are boxes and extruded circles. An `IfcRelConnectsElements`
  ties the assembly to its member.
- Import keeps such assemblies as components, which is the existing
  fallback. Round-tripping connections natively is out of scope.

## Views and app

- **Views.** Plans, sections and 3D views draw the solids like any other
  structural element (the category is structural). Visibility and
  graphics work by category.
- **Ribbon.**
  - Steel panel: a *Connections* group with Base Plates, End Plates and
    Fin Plates.
  - Timber panel: *Connections* with Bolted Joints and Hangers.
  - Each button places connections on the selected members, else on the
    active level's members, with default parameters. The result is
    reported in the status bar.
- **Properties palette.** For a selected connection, rows for its kind
  and parameters.
- **i18n.** New text in 12 languages. An icon per kind, or one shared
  `connection` icon with badges.

## Testing

- **cc_geom:**
  - BasePlate box size and position under a column;
  - anchor count rows × columns;
  - the EndPlate normal follows a turned beam;
  - FinPlate length from the bolt pattern;
  - TimberBolts length;
  - Hanger parts;
  - a connection on a moved column moves with it;
  - a missing member builds nothing.
- **Engine:**
  - `connections_create` places base plates on every column and skips
    on a second run;
  - end plates go only on beam ends at columns;
  - one undo step;
  - `element_set` refuses bad parameters;
  - repair drops a second connection on the same end;
  - deleting a column removes its base plate.
- **IFC:** export has an assembly with plates and fasteners, and the
  validator passes.
- **Schedule:** plate mass equals volume × 7850.
- **App:** the Base Plates button places connections on the selected
  columns.

## Decisions made while planning

- **Defaults.** Each kind has its own defaults for thickness, bolt
  diameter, rows × columns, spacing and edge distance. The anchor length
  is 0.40 for all. The table is in the plan's Global Constraints.
- **Bolt layout.**
  - Base and end plates spread their bolts evenly between the edge
    distances, so `spacing` is unused for them.
  - Fin plates and bolted joints place bolt columns at `edge + i·spacing`.
  - Hanger bolts pass through both sides.
- **Bolt lengths.**
  - Anchors run from the plate top to `anchor_length` below it.
  - End and fin plate bolts are t + 20 mm.
  - Timber bolts are the beam width + 2 washers. Washers are `thickness`
    thick, with a diameter of 3·d.
  - Hanger bolts are the inside width + 2t + 20 mm.
- **End plates on sloped beams.** The end plate stands vertical, normal
  to the beam's plan axis.
- **Hanger shape.** The hanger is one bent plate made of three boxes,
  exported as one `IfcPlate`.
- **Warnings.** There are two warnings: `MissingMember` and `Unsuitable`.
  Either one builds nothing.
- **Steel material.** Connections use the first Metal material, else a
  fixed gray.
- **`connections_create` member default.** Without members it works on
  `level`, else the active level, else every level. Explicit members of
  the wrong category are refused.
- **Patching parameters.** `element_set` patches
  `{"params": {"params": {...}}}`. Changing the member, end or kind is
  refused.
- **Schedule fields.** The new fields are ConnectionKind, Host,
  PlateMass, BoltCount, BoltDiameter and Volume.
- **Copies and lifecycle.**
  - Connections are never copied.
  - A connection takes its member's level, phase and option.
  - There is no format bump.
