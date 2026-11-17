//! Defaults applied when elements are created (shared by the drawing tools).

use cc_core::{Element, LevelId, LocationLine, Project, Wall, WallTop};
use cc_geom::polygon;
use glam::DVec2;

/// Top constraint for new columns and stairs: the level above, if any.
pub fn default_top(project: &Project, level: LevelId) -> WallTop {
    match project.level_above(level) {
        Some(above) => WallTop::Level { level: above.id, offset: 0.0 },
        None => WallTop::Unconnected,
    }
}

/// Default roof base: the top of the tallest wall on the level (else a
/// story: 3 m, 10'-0" in imperial projects).
pub fn default_roof_base(project: &Project, level: LevelId) -> f64 {
    project
        .walls()
        .filter(|(e, _)| e.level == level)
        .map(|(e, w)| project.wall_top_z(e, w) - project.level_elevation(level))
        .fold(None, |m: Option<f64>, h| Some(m.map_or(h, |m| m.max(h))))
        .unwrap_or(project.defaults().story)
}

/// Elevation of a new pipe's center line above its level: supply pipes run
/// in the walls (0.6 m up, 2'-0" in imperial projects), drains under the
/// floor (0.3 m down, 1'-0").
pub fn pipe_offset(project: &Project, kind: Option<cc_core::SystemKind>) -> f64 {
    use cc_core::SystemKind as K;
    let (supply, drain) = project.defaults().pipes;
    match kind {
        Some(K::ColdWater | K::HotWater | K::Vent) => supply,
        Some(K::Sanitary | K::Rainwater) => drain,
        _ => 0.0,
    }
}

/// Minimum fall of a horizontal drain of outside `diameter`, as in common
/// drainage practice: 2 % up to 75 mm, 1 % from 100 mm (in imperial
/// projects 1/4" per foot up to 3", 1/8" per foot from 4").
pub fn drain_slope(project: &Project, diameter: f64) -> f64 {
    let (small, small_fall, fall) = project.defaults().drain_fall;
    if diameter <= small { small_fall } else { fall }
}

/// Elevation of a new conduit above its level: in the slab above (the level
/// height, else 2.80 m; 9'-0" in imperial projects).
pub fn conduit_offset(project: &Project, level: LevelId) -> f64 {
    project.level_height(level, project.defaults().ceiling)
}

/// Elevation of a new cable tray's center line above its level: under the
/// slab above, clear of it by 0.3 m (1'-0" in imperial projects).
pub fn tray_offset(project: &Project, level: LevelId) -> f64 {
    let d = project.defaults();
    project.level_height(level, d.ceiling) - d.tray_clearance
}

/// Usual mounting height of an electrical device above its level (lights:
/// the ceiling, at the level height); round inches in imperial projects
/// (see [`cc_core::Defaults::devices`]).
pub fn device_height(project: &Project, level: LevelId, device: cc_core::DeviceKind) -> f64 {
    use cc_core::DeviceKind as D;
    let d = project.defaults();
    let [outlet, switch, wall_light, panel, data_outlet, rack] = d.devices;
    match device {
        D::Outlet | D::DedicatedOutlet => outlet,
        D::Switch => switch,
        D::WallLight => wall_light,
        D::Panel => panel,
        D::DataOutlet => data_outlet,
        D::Rack => rack,
        // Placing commands put air terminals on the roof under them.
        D::AirTerminal => project.level_height(level, d.ceiling),
        D::EarthElectrode => -cc_core::lightning::EARTH_DEPTH,
        D::CeilingLight => project.level_height(level, d.ceiling),
    }
}

/// Next free number among `existing` names (1 when none is numeric).
pub fn next_number<'a>(existing: impl Iterator<Item = &'a str>) -> String {
    (existing.filter_map(|s| s.trim().parse::<u32>().ok()).max().unwrap_or(0) + 1).to_string()
}

/// Closed loop around the walls of `level`, offset to the exterior faces
/// (`outward > 0`) or interior faces (`outward < 0`): the outer boundary of
/// their location lines, so interior walls (even those meeting its corners)
/// and walls hanging off it are left out; with several separate loops, the
/// largest. The offset is half the thickest wall on the boundary.
pub fn walls_loop(project: &Project, level: LevelId, outward: f64) -> Option<Vec<DVec2>> {
    let walls: Vec<(&Element, &Wall)> = project.walls().filter(|(e, _)| e.level == level).collect();
    // Ends closer than this meet (drawn without snapping).
    const TOL: f64 = 0.01;
    let mut ends: Vec<DVec2> = Vec::new();
    let mut snap = |p: DVec2| match ends.iter().find(|q| q.distance(p) < TOL) {
        Some(q) => *q,
        None => {
            ends.push(p);
            p
        }
    };
    let lines: Vec<Vec<DVec2>> = walls
        .iter()
        .map(|(_, w)| {
            let (a, b) = (snap(w.start), snap(w.end));
            let mut pts = vec![a];
            pts.extend(along(w, true));
            if let Some(last) = pts.last_mut() {
                *last = b;
            }
            pts
        })
        .collect();
    let outline = cc_geom::outer_boundary(&lines)?;
    // The walls whose middle lies on the outline set the offset.
    let on_outline = |p: DVec2| {
        (0..outline.len()).any(|i| {
            let (q, _) = polygon::closest_on_segment(outline[i], outline[(i + 1) % outline.len()], p);
            q.distance(p) < TOL
        })
    };
    let half = walls
        .iter()
        .zip(&lines)
        .filter(|(_, line)| line.len() >= 2 && on_outline(mid_point(line)))
        .map(|((e, _), _)| project.thickness_of(e) * 0.5)
        .fold(0.0, f64::max);
    Some(polygon::offset(&outline, half * outward))
}

/// Closed loop around the walls of `level` along one `face` of each wall on
/// it (issue #311): the outer boundary of their location lines, as
/// [`walls_loop`] finds it, with every side moved to that face of its own
/// wall, whatever its thickness, layers and location line. *Exterior* is
/// the side of a wall outside the loop and *interior* the side inside it
/// (not the wall's own exterior side, which depends on how it was drawn):
/// the finish faces are the wall's two faces, the core faces those of its
/// structure layers (the finish faces of a wall with no other layer).
/// Where walls in a line differ, the loop steps from one face to the next.
pub fn walls_loop_at(project: &Project, level: LevelId, face: LocationLine) -> Option<Vec<DVec2>> {
    let walls: Vec<(&Element, &Wall)> = project.walls().filter(|(e, _)| e.level == level).collect();
    const TOL: f64 = 0.01;
    let mut ends: Vec<DVec2> = Vec::new();
    let mut snap = |p: DVec2| match ends.iter().find(|q| q.distance(p) < TOL) {
        Some(q) => *q,
        None => {
            ends.push(p);
            p
        }
    };
    let lines: Vec<Vec<DVec2>> = walls
        .iter()
        .map(|(_, w)| {
            let (a, b) = (snap(w.start), snap(w.end));
            let mut pts = vec![a];
            pts.extend(along(w, true));
            if let Some(last) = pts.last_mut() {
                *last = b;
            }
            pts
        })
        .collect();
    let outline = polygon::ccw(&cc_geom::outer_boundary(&lines)?);
    let n = outline.len();
    if n < 3 {
        return None;
    }
    // How far outside the loop the face of the wall under each side lies
    // from the side (the wall's location line).
    let distance = |a: DVec2, b: DVec2| {
        let (mid, dir) = ((a + b) * 0.5, (b - a).normalize_or_zero());
        for ((e, w), line) in walls.iter().zip(&lines) {
            for seg in line.windows(2) {
                let along = (seg[1] - seg[0]).normalize_or_zero();
                if polygon::closest_on_segment(seg[0], seg[1], mid).0.distance(mid) > TOL || along.dot(dir).abs() < 0.5
                {
                    continue;
                }
                let Some(ty) = project.element_type(e.type_id) else { return 0.0 };
                // A wall's left side is outside a counter-clockwise loop when it runs against it.
                let side = if along.dot(dir) < 0.0 { 1.0 } else { -1.0 };
                let half = ty.thickness() * 0.5;
                let (core_left, core_right) = ty.core_range();
                // Faces across the wall, from its centerline towards its left side.
                let (left, right) = (half - core_left, half - core_right);
                let outside = |towards_left: f64| towards_left * side;
                let (core_out, core_in) = if side > 0.0 { (left, right) } else { (right, left) };
                let at = match face {
                    LocationLine::Centerline => 0.0,
                    LocationLine::CoreCenter => outside((left + right) * 0.5),
                    LocationLine::FinishExterior => half,
                    LocationLine::FinishInterior => -half,
                    LocationLine::CoreExterior => outside(core_out),
                    LocationLine::CoreInterior => outside(core_in),
                };
                return at - outside(ty.location_offset(w.location_line));
            }
        }
        0.0
    };
    let sides: Vec<(DVec2, DVec2, f64)> = (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            // Outside a counter-clockwise loop is to the right of each side.
            let out = -(b - a).normalize_or_zero().perp();
            let d = distance(a, b);
            (a + out * d, (b - a).normalize_or_zero(), d)
        })
        .collect();
    let mut pts: Vec<DVec2> = Vec::with_capacity(n);
    for i in 0..n {
        let (prev, cur) = (sides[(i + n - 1) % n], sides[i]);
        let corner = outline[i];
        let (out_prev, out_cur) = (-prev.1.perp(), -cur.1.perp());
        if prev.1.perp_dot(cur.1).abs() > 1e-6 {
            // Where the two moved sides cross (a miter no longer than five times the offset).
            match polygon::line_intersect(prev.0, prev.1, cur.0, cur.1) {
                Some(t) if (prev.0 + prev.1 * t).distance(corner) <= 5.0 * prev.2.abs().max(cur.2.abs()) + TOL => {
                    pts.push(prev.0 + prev.1 * t)
                }
                _ => pts.extend([corner + out_prev * prev.2, corner + out_cur * cur.2]),
            }
        } else if (prev.2 - cur.2).abs() < 1e-9 {
            pts.push(corner + out_cur * cur.2);
        } else {
            // Walls in a line with faces apart: a step.
            pts.extend([corner + out_prev * prev.2, corner + out_cur * cur.2]);
        }
    }
    pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
    (pts.len() >= 3 && pts.iter().all(|p| p.is_finite())).then_some(pts)
}

/// The face a loop of walls follows when none is asked for: the outside
/// faces, or the inside ones for ceilings.
pub fn default_wall_face(category: cc_core::Category) -> LocationLine {
    if category == cc_core::Category::Ceiling { LocationLine::FinishInterior } else { LocationLine::FinishExterior }
}

/// The point halfway along a polyline.
fn mid_point(line: &[DVec2]) -> DVec2 {
    let total: f64 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * 0.5;
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        if len >= left && len > 0.0 {
            return w[0].lerp(w[1], left / len);
        }
        left -= len;
    }
    line.first().copied().unwrap_or_default()
}

/// Points of a wall's location line after its first end (`forward`: from
/// start to end): its other end, preceded by points along an arc.
fn along(w: &Wall, forward: bool) -> Vec<DVec2> {
    let mut pts = match w.arc() {
        Some(a) => a.points(0.0, a.length(), 0.0, cc_core::arc::ARC_STEP),
        None => vec![w.start, w.end],
    };
    if !forward {
        pts.reverse();
    }
    pts.remove(0);
    pts
}
