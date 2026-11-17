# Component modeling

*Português: [modelagem-de-componentes.md](modelagem-de-componentes.md).*

Components (furniture, fixtures, casework, doors, windows…) are parametric definitions: named parameters and pieces whose coordinates are expressions. The **component modeling environment** shapes them visually, and every step is also an engine command, so a component can be built from a script, the `ccad` command line or MCP just as well as by hand.

## The environment

*Edit Component* (*Architecture → Model*, or *Edit Component…* in a component type's properties) opens a workspace of its own. The project's views give way to the component's **Plan**, **Front**, **Side** and **3D** views. The ribbon shows the **Create** tab, and the left panel shows the component's types, its parameters and the properties of what is selected. *Finish* keeps the changes; *Cancel Editing* undoes everything done since the environment opened. *New Component* (*Architecture → Model*) asks for a name and the kind of the component (furniture, casework, plumbing fixture, lighting, electrical, generic, door or window) and shows the parameters that kind starts with: width, depth and height for most, a diameter for lighting, the opening's width, height and sill for doors and windows; a generic component starts with none, and its parameters are yours to add. *Create* makes the component and opens it here; *Cancel Editing* removes it again, and *Finish* picks the tool that places it. See [interface.md](interface.md#component-modeling-environment) for the tools, clicks and keys.

## How the model works

Everything is an expression of the component (`cc_core::expr`): there is no constraint solver. Three ideas build on that.

- **Reference planes** (`ComponentDef::ref_planes`) are named planes square to X (left/right), Y (front/back) or Z (heights), at a position that is an expression. Three origin planes are always there at zero: `CenterX`, `CenterY` and `Base`. Plane names are values in every expression of the component, so planes may read parameters and each other, in any order; cycles and unknown names are reported.
- **Locking** a piece to a plane means writing the plane's name in the piece's expression (`x = Left`, `start = Under`, `end = Top`). The piece then follows the plane wherever parameters move it.
- **Dimensions** between parallel planes (`dimensions`) show their distance. Labeling one with a parameter rewrites the moving plane as the other plus or minus the parameter (`Right = Left + Width`), so the parameter drives it. **Equalities** (`equals`, EQ) write the middle plane as `(a + b) / 2`.

Because everything is stored as expressions, older `.ccomp` files and projects load unchanged, and the new fields are omitted when empty.

## Pieces

| Shape | Made of | Engine `kind` |
|---|---|---|
| Extrusion | a closed outline on the plan, front or side plane, from `start` to `end` along the plane's normal | `extrusion` |
| Revolution | a closed outline turned `angle` degrees about the line u = `axis` of its plane | `revolution` |
| Sweep | a closed section (u to the left of the path, v along the normal) carried along an open path of lines and arcs, mitered at the corners | `sweep` |
| Blend | a bottom outline at `start` joined to a top outline at `end`, both resampled evenly so different shapes line up | `blend` |

Any piece can be a **void**, which is cut out of the solid pieces it overlaps. Outline points may carry a `radius` that makes the segment to the next point an arc (positive bulging outwards). Boxes, cylinders, plan extrusions and imported meshes from older definitions keep working.

## Commands

All of these take `type`, any type of the component (by id or name). A change goes to every type of the component in one undo step, and each type keeps its own parameter values.

| Command | Does |
|---|---|
| `component_get` | the definition, every plane's value, dimension values, problems and the types with their values |
| `component_param_add` / `_set` / `_delete` | parameters; a rename reaches every expression and dimension; one in use cannot be deleted |
| `component_ref_plane_add` / `_set` / `_delete` | reference planes (origin planes cannot move or go) |
| `component_ref_line_add` / `_delete` | reference lines on a sketch plane |
| `component_dimension_add` / `_set` / `_delete` | dimensions between planes, their label, value and place |
| `component_equal_add` / `_delete` | equalities (EQ) |
| `component_piece_add` / `_set` / `_delete` | sketched pieces: extrusion, revolution, sweep, blend, solid or void |

[`examples/component_bench.ccmacro`](../examples/component_bench.ccmacro) models a parametric bench with these commands only: `ccad --save-as bench.ccad script examples/component_bench.ccmacro`. The full reference is in [commands.md](commands.md).

## Code map

- `cc_core::component` (definitions, pieces, resolution), `cc_core::component_ref` (planes, lines, dimensions, equalities, the `Scope` of names), `cc_core::profile` (outlines, extrusions, revolutions), `cc_core::sweep` (sweeps and blends).
- `cc_engine::commands::component` and `component_piece` hold the commands, with tests in `crates/cc_engine/tests/component_env.rs`.
- `cc_views::component_view` draws the four views and lists the planes, dimensions, equalities and piece edges for picking.
- `cc_app::component_env` (workspace, panel, picking, plane/dimension/EQ tools) and `cc_app::component_sketch` (sketch tools, snapping and locking).

## Not yet

- **Nested components:** a piece that is another component, with its parameters mapped. This needs pieces to resolve against the project's other definitions.
- **Visibility by view or detail level:** pieces shown only in some views or at some levels of detail. The views have no detail level yet.
- **Constraints inside a sketch** (parallel, tangent, equal segments): points are expressions locked to planes, not solved.
- **Dragging faces and planes in the views:** edits are made with the tools and in the properties for now.
