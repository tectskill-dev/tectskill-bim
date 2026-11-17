//! Doors and windows as components: the built-in definitions that the
//! template's door and window types use and that older files' door and
//! window types become.
//!
//! A door or window component ([`ComponentKind::Door`] or `Window`, with a
//! [`HostOpening`]) is placed in a wall: its frame has `x` along the wall
//! from the opening's center, `y` across the wall from its location line
//! (positive on the side a door opens to) and `z` up from the sill; its
//! expressions may use [`WALL_THICKNESS`], the host wall's thickness.

use crate::component::{
    ComponentDef, ComponentItem, ComponentKind, ComponentParam, ComponentShape, DoorOperation, HostOpening, ParamKind,
    WALL_THICKNESS, WindowOperation,
};
use crate::{Project, TypeData};

/// Width of frame members (jambs, head, sash rails), meters.
pub const FRAME: f64 = 0.05;
/// Width of the stiles and top rail of a glazed door's leaves, meters.
pub const STILE: f64 = 0.08;
/// Height of a French door's bottom rail (the solid lower part of each leaf), meters.
pub const BOTTOM_RAIL: f64 = 0.4;

/// Names of the built-in parameters and pieces, in the project's language.
#[derive(Debug, Clone)]
pub struct OpeningNames {
    pub door: String,
    /// Swing doors of two leaves.
    pub double_door: String,
    /// Glazed doors (see [`french_door_def`]), of one leaf and of two.
    pub french_door: String,
    pub french_door_double: String,
    pub window: String,
    /// The component of niches (see [`niche_def`]).
    pub niche: String,
    /// The depth of a niche.
    pub depth: String,
    /// The components of windows that open (see [`Self::window_for`]).
    pub casement: String,
    pub double_casement: String,
    pub top_hung: String,
    pub pivot_horizontal: String,
    pub pivot_vertical: String,
    pub sliding: String,
    pub width: String,
    pub height: String,
    pub sill: String,
    /// The height of a French door's bottom rail.
    pub bottom_rail: String,
    pub frame: String,
    pub leaf: String,
    pub sash: String,
    pub glass: String,
    pub sill_board: String,
}

impl Default for OpeningNames {
    /// The Portuguese names (the template's defaults).
    fn default() -> Self {
        Self::new(&|key| crate::project::template_default(key).to_string())
    }
}

impl OpeningNames {
    /// Names from `n(key)` (template keys `tpl-*`); parameter names that are
    /// not valid in expressions fall back to the Portuguese ones.
    pub fn new(n: &dyn Fn(&str) -> String) -> Self {
        let param = |key: &str| {
            let name = n(key);
            if crate::expr::is_valid_name(&name) && name != WALL_THICKNESS {
                name
            } else {
                crate::project::template_default(key).to_string()
            }
        };
        Self {
            door: n("tpl-comp-door"),
            double_door: n("tpl-comp-door-double"),
            french_door: n("tpl-comp-french-door"),
            french_door_double: n("tpl-comp-french-door-double"),
            window: n("tpl-comp-window"),
            niche: n("tpl-comp-niche"),
            depth: param("tpl-param-depth"),
            casement: n("tpl-comp-window-casement"),
            double_casement: n("tpl-comp-window-double"),
            top_hung: n("tpl-comp-window-top-hung"),
            pivot_horizontal: n("tpl-comp-window-pivot-h"),
            pivot_vertical: n("tpl-comp-window-pivot-v"),
            sliding: n("tpl-comp-window-sliding"),
            width: param("tpl-param-width"),
            height: param("tpl-param-height"),
            sill: param("tpl-param-sill"),
            bottom_rail: param("tpl-param-bottom-rail"),
            frame: n("tpl-piece-frame"),
            leaf: n("tpl-piece-leaf"),
            sash: n("tpl-piece-sash"),
            glass: n("tpl-piece-glass"),
            sill_board: n("tpl-piece-sill-board"),
        }
    }
}

impl OpeningNames {
    /// The component of windows that open as `operation` says.
    pub fn window_for(&self, operation: WindowOperation) -> &String {
        match operation {
            WindowOperation::Fixed => &self.window,
            WindowOperation::SideHungLeft | WindowOperation::SideHungRight => &self.casement,
            WindowOperation::DoubleSideHung => &self.double_casement,
            WindowOperation::TopHung => &self.top_hung,
            WindowOperation::PivotHorizontal => &self.pivot_horizontal,
            WindowOperation::PivotVertical => &self.pivot_vertical,
            WindowOperation::Sliding => &self.sliding,
        }
    }

    /// The component of doors whose leaves swing as `operation` says,
    /// glazed (French doors) or not.
    pub fn door_for(&self, operation: DoorOperation, glazed: bool) -> &String {
        match (operation.leaves(), glazed) {
            (2, true) => &self.french_door_double,
            (_, true) => &self.french_door,
            (2, false) => &self.double_door,
            _ => &self.door,
        }
    }
}

fn length(name: &str, value: f64) -> ComponentParam {
    ComponentParam { name: name.to_string(), value, kind: ParamKind::Length, instance: false, ..Default::default() }
}

/// A box piece from its corner and sizes (expressions).
fn piece(name: &str, material: &str, [x, y, z, width, depth, height]: [String; 6]) -> ComponentItem {
    ComponentItem {
        name: name.to_string(),
        shape: ComponentShape::Box { x, y, z, width, depth, height },
        material: material.to_string(),
        in_plan: false,
        void: false,
        plan_only: false,
        detail: Default::default(),
    }
}

/// A swing door: two jambs and a head as deep as the wall, and a leaf of
/// `material` in the middle of the wall.
pub fn door_def(names: &OpeningNames, width: f64, height: f64, material: &str) -> ComponentDef {
    let (w, h, t) = (&names.width, &names.height, WALL_THICKNESS);
    let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{T}", t).replace("{F}", &FRAME.to_string());
    let frame = &names.frame;
    let items = vec![
        piece(frame, "", [s("-{W} / 2"), s("-{T} / 2"), s("0"), s("{F}"), s("{T}"), s("{H}")]),
        piece(frame, "", [s("{W} / 2 - {F}"), s("-{T} / 2"), s("0"), s("{F}"), s("{T}"), s("{H}")]),
        piece(frame, "", [s("-{W} / 2 + {F}"), s("-{T} / 2"), s("{H} - {F}"), s("{W} - 2 * {F}"), s("{T}"), s("{F}")]),
        piece(
            &names.leaf,
            material,
            [s("-{W} / 2 + {F}"), s("-0.02"), s("0"), s("{W} - 2 * {F}"), s("0.04"), s("{H} - {F}")],
        ),
    ];
    ComponentDef {
        kind: ComponentKind::Door,
        params: vec![length(w, width), length(h, height)],
        items,
        opening: Some(Box::new(HostOpening {
            width: w.clone(),
            height: h.clone(),
            sill: String::new(),
            own_symbol: false,
            operation: WindowOperation::Fixed,
            door_operation: DoorOperation::SingleSwingLeft,
            depth: String::new(),
        })),
        ..Default::default()
    }
}

/// A swing door whose leaves open as `operation` says: [`door_def`] with,
/// for a double door, two leaves meeting in the middle.
pub fn operable_door_def(
    names: &OpeningNames,
    (width, height): (f64, f64),
    material: &str,
    operation: DoorOperation,
) -> ComponentDef {
    let mut def = door_def(names, width, height, material);
    if let Some(o) = def.opening.as_mut() {
        o.door_operation = operation;
    }
    if operation.leaves() == 2 {
        let (w, h) = (&names.width, &names.height);
        let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{F}", &FRAME.to_string());
        def.items.retain(|i| i.name != names.leaf);
        for x in [s("-{W} / 2 + {F}"), s("0")] {
            let leaf = [x, s("-0.02"), s("0"), s("{W} / 2 - {F}"), s("0.04"), s("{H} - {F}")];
            def.items.push(piece(&names.leaf, material, leaf));
        }
    }
    def
}

/// A French door: a glazed swing door from the floor, whose leaves (one or
/// two, as `operation` says) are stiles and a top rail of `leaf_material`
/// around a pane of `glass`, over a solid bottom rail `bottom_rail` tall
/// (the [`OpeningNames::bottom_rail`] parameter), in a frame as deep as the wall.
pub fn french_door_def(
    names: &OpeningNames,
    (width, height, bottom_rail): (f64, f64, f64),
    (leaf_material, glass): (&str, &str),
    operation: DoorOperation,
) -> ComponentDef {
    let mut def = door_def(names, width, height, leaf_material);
    def.items.retain(|i| i.name != names.leaf);
    def.params.push(length(&names.bottom_rail, bottom_rail));
    if let Some(o) = def.opening.as_mut() {
        o.door_operation = operation;
    }
    let (w, h, b) = (&names.width, &names.height, &names.bottom_rail);
    let s = |text: &str| {
        text.replace("{W}", w)
            .replace("{H}", h)
            .replace("{B}", b)
            .replace("{F}", &FRAME.to_string())
            .replace("{S}", &STILE.to_string())
    };
    let leaves: Vec<(String, String)> = if operation.leaves() == 2 {
        vec![(s("-{W} / 2 + {F}"), s("{W} / 2 - {F}")), (s("0"), s("{W} / 2 - {F}"))]
    } else {
        vec![(s("-{W} / 2 + {F}"), s("{W} - 2 * {F}"))]
    };
    let (depth, y) = (s("0.05"), s("-0.025"));
    for (x, l) in leaves {
        let inner_x = format!("{x} + {}", STILE);
        let inner = format!("{l} - {}", 2.0 * STILE);
        let members = [
            [x.clone(), y.clone(), s("0"), s("{S}"), depth.clone(), s("{H} - {F}")],
            [format!("{x} + {l} - {STILE}"), y.clone(), s("0"), s("{S}"), depth.clone(), s("{H} - {F}")],
            [inner_x.clone(), y.clone(), s("0"), inner.clone(), depth.clone(), s("{B}")],
            [inner_x.clone(), y.clone(), s("{H} - {F} - {S}"), inner.clone(), depth.clone(), s("{S}")],
        ];
        for member in members {
            def.items.push(piece(&names.leaf, leaf_material, member));
        }
        let pane = [inner_x, s("-0.006"), s("{B}"), inner, s("0.012"), s("{H} - {F} - {S} - {B}")];
        def.items.push(piece(&names.glass, glass, pane));
    }
    def
}

/// A window: a sash frame in the middle of the wall around a pane of
/// `material`, and a sill board across the wall under it.
pub fn window_def(names: &OpeningNames, width: f64, height: f64, sill: f64, material: &str) -> ComponentDef {
    let (w, h, t) = (&names.width, &names.height, WALL_THICKNESS);
    let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{T}", t).replace("{F}", &FRAME.to_string());
    let sash = &names.sash;
    let items = vec![
        piece(sash, "", [s("-{W} / 2"), s("-0.04"), s("0"), s("{F}"), s("0.08"), s("{H}")]),
        piece(sash, "", [s("{W} / 2 - {F}"), s("-0.04"), s("0"), s("{F}"), s("0.08"), s("{H}")]),
        piece(sash, "", [s("-{W} / 2 + {F}"), s("-0.04"), s("0"), s("{W} - 2 * {F}"), s("0.08"), s("{F}")]),
        piece(sash, "", [s("-{W} / 2 + {F}"), s("-0.04"), s("{H} - {F}"), s("{W} - 2 * {F}"), s("0.08"), s("{F}")]),
        piece(
            &names.sill_board,
            "",
            [s("-{W} / 2"), s("-{T} / 2 - 0.02"), s("-0.02"), s("{W}"), s("{T} + 0.02"), s("0.02")],
        ),
        piece(
            &names.glass,
            material,
            [s("-{W} / 2 + {F}"), s("-0.006"), s("{F}"), s("{W} - 2 * {F}"), s("0.012"), s("{H} - 2 * {F}")],
        ),
    ];
    ComponentDef {
        kind: ComponentKind::Window,
        params: vec![length(w, width), length(h, height), length(&names.sill, sill)],
        items,
        opening: Some(Box::new(HostOpening {
            width: w.clone(),
            height: h.clone(),
            sill: names.sill.clone(),
            own_symbol: false,
            operation: WindowOperation::Fixed,
            door_operation: DoorOperation::SingleSwingLeft,
            depth: String::new(),
        })),
        ..Default::default()
    }
}

/// A niche: a recess `width` wide, `height` tall and `depth` deep in one
/// face of its wall, its bottom `sill` above the wall's base. It has no
/// pieces: the wall shows inside it (pieces of a niche component are not
/// placed yet).
pub fn niche_def(names: &OpeningNames, (width, height, depth, sill): (f64, f64, f64, f64)) -> ComponentDef {
    let (w, h, d) = (&names.width, &names.height, &names.depth);
    ComponentDef {
        kind: ComponentKind::Niche,
        params: vec![length(w, width), length(h, height), length(d, depth), length(&names.sill, sill)],
        items: Vec::new(),
        opening: Some(Box::new(HostOpening {
            width: w.clone(),
            height: h.clone(),
            sill: names.sill.clone(),
            own_symbol: false,
            operation: WindowOperation::Fixed,
            door_operation: DoorOperation::SingleSwingLeft,
            depth: d.clone(),
        })),
        ..Default::default()
    }
}

/// The least a niche leaves of its wall behind it, meters: a deeper one
/// stops there (an opening through the wall is a door or a window).
pub const NICHE_MIN_BACK: f64 = 0.01;

/// A window whose panels open as `operation` says: [`window_def`] with,
/// for two side-hung panels, a stile where they meet and a pane in each,
/// and for sliding ones two panels on either side of the wall's middle,
/// overlapping by a stile.
pub fn operable_window_def(
    names: &OpeningNames,
    (width, height, sill): (f64, f64, f64),
    material: &str,
    operation: WindowOperation,
) -> ComponentDef {
    let mut def = window_def(names, width, height, sill, material);
    if let Some(o) = def.opening.as_mut() {
        o.operation = operation;
    }
    let (w, h) = (&names.width, &names.height);
    let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{F}", &FRAME.to_string());
    let tall = s("{H} - 2 * {F}");
    match operation {
        WindowOperation::DoubleSideHung => {
            def.items.retain(|i| i.name != names.glass);
            let stile = [s("-{F}"), s("-0.04"), s("{F}"), s("2 * {F}"), s("0.08"), tall.clone()];
            def.items.push(piece(&names.sash, "", stile));
            for x in [s("-{W} / 2 + {F}"), s("{F}")] {
                let pane = [x, s("-0.006"), s("{F}"), s("{W} / 2 - 2 * {F}"), s("0.012"), tall.clone()];
                def.items.push(piece(&names.glass, material, pane));
            }
        }
        WindowOperation::Sliding => {
            def.items.retain(|i| i.name != names.glass);
            // Each panel is half the inner width plus half a stile, on its own track.
            for (stiles, glass_x, track) in [
                ([s("-{W} / 2 + {F}"), s("-{F} / 2")], s("-{W} / 2 + 2 * {F}"), -0.035),
                ([s("-{F} / 2"), s("{W} / 2 - 2 * {F}")], s("{F} / 2"), 0.005),
            ] {
                for x in stiles {
                    let stile = [x, track.to_string(), s("{F}"), s("{F}"), "0.03".into(), tall.clone()];
                    def.items.push(piece(&names.sash, "", stile));
                }
                let y = (track + 0.009).to_string();
                let pane = [glass_x, y, s("{F}"), s("{W} / 2 - 5 * {F} / 2"), "0.012".into(), tall.clone()];
                def.items.push(piece(&names.glass, material, pane));
            }
        }
        _ => {}
    }
    def
}

impl Project {
    /// Types of parametric components, of any category (doors and windows too).
    pub fn component_types(&self) -> impl Iterator<Item = &crate::ElementType> {
        self.types.values().filter(|t| matches!(t.data, TypeData::Component { .. }))
    }

    /// Turn door and window types of the old kind (fixed width, height and
    /// material) into types of the built-in door and window components, with
    /// the same sizes; elements keep their types. Returns how many changed.
    pub fn convert_opening_types(&mut self, n: &dyn Fn(&str) -> String) -> usize {
        let names = OpeningNames::new(n);
        let mut changed = 0;
        for ty in self.types.values_mut() {
            let converted = match &ty.data {
                TypeData::Door { width, height, material } => Some(TypeData::Component {
                    component: names.door.clone(),
                    def: door_def(&names, *width, *height, material),
                }),
                TypeData::Window { width, height, default_sill, material } => Some(TypeData::Component {
                    component: names.window.clone(),
                    def: window_def(&names, *width, *height, *default_sill, material),
                }),
                _ => None,
            };
            if let Some(data) = converted {
                ty.data = data;
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, ElementType, ElementTypeId};

    fn names() -> OpeningNames {
        OpeningNames::new(&|key| crate::project::template_default(key).to_string())
    }

    #[test]
    fn casement_windows_resolve_and_map_to_ifc_panels() {
        let names = names();
        for op in WindowOperation::ALL {
            let def = operable_window_def(&names, (1.2, 1.2, 1.0), "Vidro", op);
            let (parts, errors) = def.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
            assert!(errors.is_empty(), "{op:?}: {errors:?}");
            let panes = def.items.iter().filter(|i| i.name == names.glass).count();
            let two = matches!(op, WindowOperation::DoubleSideHung | WindowOperation::Sliding);
            assert_eq!(panes, if two { 2 } else { 1 });
            assert!(!parts.is_empty());
            assert_eq!(def.opening.as_ref().map(|o| o.operation), Some(op));
            let panels: Vec<&str> = op.ifc_panels().iter().map(|(o, _)| *o).collect();
            assert_eq!(WindowOperation::from_ifc_panels(&panels), Some(op));
            let side_hung = matches!(
                op,
                WindowOperation::SideHungLeft | WindowOperation::SideHungRight | WindowOperation::DoubleSideHung
            );
            assert_eq!(op.hinges().len(), if side_hung { panels.len() } else { 0 });
            assert_eq!(op.elevation_lines(1.2, 1.2).is_empty(), op == WindowOperation::Fixed);
        }
        assert_eq!(WindowOperation::from_ifc_panels(&["TILTANDTURNLEFTHAND"]), None);
        // Fixed windows keep files as they were: no operation is written.
        let fixed = ron::to_string(&window_def(&names, 1.2, 1.2, 1.0, "Vidro")).unwrap();
        assert!(!fixed.contains("operation"));
    }

    #[test]
    fn built_in_doors_and_windows_are_openings_of_their_category() {
        let names = names();
        let door = door_def(&names, 0.8, 2.1, "Madeira");
        assert_eq!(door.opening_size(), Some((0.8, 2.1)));
        assert_eq!(door.default_sill(), 0.0);
        let window = window_def(&names, 1.2, 1.0, 1.1, "Vidro");
        assert_eq!(window.opening_size(), Some((1.2, 1.0)));
        assert!((window.default_sill() - 1.1).abs() < 1e-12);
        let ty = |def: ComponentDef| ElementType {
            id: ElementTypeId(1),
            name: "t".into(),
            data: TypeData::Component { component: "c".into(), def },
            info: Default::default(),
            uid: Default::default(),
        };
        assert_eq!(ty(door.clone()).category(), Category::Door);
        assert_eq!(ty(window.clone()).category(), Category::Window);
        // Every piece resolves in a wall 15 cm thick; the frame is as deep as the wall.
        let (parts, errors) = door.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(parts.len(), 4);
        let (lo, hi) = parts[0].footprint.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        assert!((hi - lo - 0.15).abs() < 1e-12);
        let (_, errors) = window.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn doors_of_one_or_two_leaves_resolve_and_map_to_ifc() {
        let names = names();
        for op in DoorOperation::ALL {
            for glazed in [false, true] {
                let def = if glazed {
                    french_door_def(&names, (1.4, 2.2, BOTTOM_RAIL), ("Madeira", "Vidro"), op)
                } else {
                    operable_door_def(&names, (1.4, 2.1), "Madeira", op)
                };
                let (parts, errors) = def.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
                assert!(errors.is_empty(), "{op:?} {glazed}: {errors:?}");
                assert!(!parts.is_empty());
                assert_eq!(def.kind, ComponentKind::Door);
                assert_eq!(def.default_sill(), 0.0);
                assert_eq!(def.opening.as_ref().map(|o| o.door_operation), Some(op));
                // One leaf piece, or one pane, per leaf.
                let per_leaf = if glazed { &names.glass } else { &names.leaf };
                assert_eq!(def.items.iter().filter(|i| &i.name == per_leaf).count(), op.leaves());
            }
            assert_eq!(DoorOperation::from_ifc(op.ifc_operation()), Some(op));
            assert_eq!(op.hinges().len(), op.leaves());
            assert_eq!(op.ifc_panels().len(), op.leaves());
            assert_eq!(op.mirrored().mirrored(), op);
        }
        assert_eq!(DoorOperation::from_ifc("SLIDING_TO_LEFT"), None);
        // The bottom rail is a parameter: the panes start on top of it.
        let def = french_door_def(&names, (0.8, 2.2, 0.5), ("Madeira", "Vidro"), DoorOperation::SingleSwingLeft);
        let (parts, _) = def.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
        let pane = parts.iter().find(|p| p.material == "Vidro").unwrap();
        assert!((pane.z0 - 0.5).abs() < 1e-9, "{}", pane.z0);
        // Plain single doors keep files as they were: no door operation is written.
        let plain = ron::to_string(&door_def(&names, 0.8, 2.1, "Madeira")).unwrap();
        assert!(!plain.contains("door_operation"));
    }

    #[test]
    fn old_door_and_window_types_become_components() {
        let mut p = Project::empty();
        let id = ElementTypeId(p.alloc_id());
        let data = TypeData::Window { width: 1.5, height: 1.0, default_sill: 1.1, material: "Vidro".into() };
        p.types
            .insert(id, ElementType { id, name: "J".into(), data, info: Default::default(), uid: Default::default() });
        assert_eq!(p.convert_opening_types(&|key| crate::project::template_default(key).to_string()), 1);
        let ty = p.element_type(id).unwrap();
        assert_eq!(ty.category(), Category::Window);
        assert_eq!(ty.opening_size(), Some((1.5, 1.0)));
        assert!((ty.default_sill() - 1.1).abs() < 1e-12);
        assert_eq!(p.convert_opening_types(&|key| crate::project::template_default(key).to_string()), 0);
    }
}
