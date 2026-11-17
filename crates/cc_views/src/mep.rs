//! Building services in plans: pipes and conduits as bands in their
//! system's color with diameter, slope and circuit tags, cable trays as
//! outlines as wide as they are, valves on pipes, and electrical devices
//! as the usual plan symbols (sizes in paper millimeters).
//!
//! Symbols follow common Brazilian practice: an outlet is a triangle,
//! hollow when low, half filled at medium height and filled when high; a
//! ceiling light is a circle with its circuit above its power; a switch is
//! a small filled circle; a board is a rectangle with a filled half.

use crate::{Anchor, Drawing2D, LineStyle, format};
use cc_core::{ConductorRole, DeviceKind, Element, ElementKind, LevelId, MepData, MepDiscipline, Project, SystemClass};
use cc_geom::ModelGeometry;
use glam::DVec2;

/// Ink of filled symbols.
const INK: [u8; 4] = [40, 40, 40, 255];
/// Outlets below this height are low, below [`MEDIUM`] medium, else high.
const LOW: f64 = 0.6;
const MEDIUM: f64 = 1.6;
/// Conduits lower than this are in the floor (drawn dashed).
const IN_FLOOR: f64 = 0.3;

/// Pipes or conduits of `level` and, in electrical plans, the device symbols.
pub(crate) fn services(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_height: f64,
    discipline: MepDiscipline,
    scale: u32,
) {
    use MepDiscipline as D;
    let mm = scale as f64 / 1000.0;
    // Cable trays first: conduits and devices are drawn over them.
    if matches!(discipline, D::Electrical | D::Communications) {
        for e in project.elements.values() {
            if let (ElementKind::CableTray(_), Some(s)) = (&e.kind, project.segment_at(e, level)) {
                tray(d, project, geom, e, &s, mm);
            }
        }
    }
    // Conduits carrying structured cabling show in communications plans.
    let cabling = |e: &Element| {
        e.systems().iter().filter_map(|s| project.system(*s)).any(|s| s.kind.class() == SystemClass::Data)
    };
    for e in project.elements.values() {
        match (&e.kind, discipline) {
            (ElementKind::Pipe(_), D::Plumbing) | (ElementKind::Conduit(_), D::Electrical) => {
                // The level's runs and the risers of other levels passing through it.
                if let Some(s) = project.segment_at(e, level) {
                    segment(d, project, geom, e, &s, cut_height, mm);
                }
            }
            (ElementKind::Conduit(_), D::Communications) if cabling(e) => {
                if let Some(s) = project.segment_at(e, level) {
                    segment(d, project, geom, e, &s, cut_height, mm);
                }
            }
            (ElementKind::LightningConductor(c), D::Lightning) => {
                if let Some(s) = project.segment_at(e, level) {
                    conductor(d, project, geom, e, &s, c.role, mm);
                }
            }
            (ElementKind::Component(c), D::Electrical | D::Communications | D::Lightning) if e.level == level => {
                let Some(MepData::Device { device, power, .. }) = project.mep_data(e) else { continue };
                if discipline_of(device.class()) != discipline {
                    continue;
                }
                // Boards and racks show the systems they feed.
                let label = if device.is_board() {
                    let fed: Vec<&str> =
                        project.systems.values().filter(|s| s.panel == Some(e.id)).map(|s| s.label()).collect();
                    (device == DeviceKind::Rack && !fed.is_empty()).then(|| fed.join(", "))
                } else {
                    c.system.and_then(|s| project.system(s)).map(|s| s.label().to_string())
                };
                let frame = Frame { origin: c.position, angle: c.rotation_deg.to_radians(), mm };
                symbol(d, &frame, device, c.base_offset, power, label.as_deref(), Some(e.id));
            }
            _ => {}
        }
    }
    let category = match discipline {
        D::Plumbing => Some(cc_core::Category::Pipe),
        D::Electrical => Some(cc_core::Category::Conduit),
        D::Lightning => Some(cc_core::Category::LightningConductor),
        D::Communications => None,
    };
    for f in geom.mep.fittings.iter().filter(|f| f.level == level && Some(f.category) == category) {
        fitting(d, project, f, mm);
    }
    if discipline == MepDiscipline::Plumbing {
        valves(d, project, geom, level, mm);
    }
}

/// The services plan that shows devices of a family of systems.
pub fn discipline_of(class: SystemClass) -> MepDiscipline {
    match class {
        SystemClass::Plumbing => MepDiscipline::Plumbing,
        SystemClass::Circuit => MepDiscipline::Electrical,
        SystemClass::Data => MepDiscipline::Communications,
        SystemClass::Lightning => MepDiscipline::Lightning,
    }
}

/// A lightning protection conductor in a lightning protection plan: roof
/// conductors as a line in the system's color (at least 0.5 mm wide on
/// paper), buried ones dashed, and a down conductor as a filled circle with
/// a diagonal stroke (the conductor going down).
fn conductor(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    e: &Element,
    s: &cc_core::MepSegment,
    role: ConductorRole,
    mm: f64,
) {
    let Some(g) = geom.segments.get(&e.id) else { return };
    let color = project.system_color(e);
    if s.is_vertical() {
        let r = 1.2 * mm;
        let pts = circle(s.start, r);
        d.fill(pts.clone(), [color[0], color[1], color[2], 255], Some(e.id));
        d.polyline(&pts, true, LineStyle::Projection, Some(e.id));
        d.line(s.start + DVec2::new(-r, r) * 1.6, s.start + DVec2::new(r, -r) * 1.6, LineStyle::Thin, Some(e.id));
        return;
    }
    if role == ConductorRole::Earth {
        d.line(s.start, s.end, LineStyle::Hidden, Some(e.id));
        return;
    }
    band(d, s.start, s.end, g.diameter.max(0.5 * mm), color, LineStyle::Projection, e);
}

/// The valves on the pipes seen from `level`: on its own runs, and on risers
/// through it between its floor and the level above.
fn valves(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, level: LevelId, mm: f64) {
    let (floor, top) =
        (project.level_elevation(level), project.level_elevation(level) + project.level_height(level, 3.0));
    for v in geom.valves.values() {
        let Some(host) = project.element(v.host) else { continue };
        let seen =
            host.level == level || (project.segment_at(host, level).is_some() && (floor..top).contains(&v.point.z));
        if !seen {
            continue;
        }
        let dir = v.dir.truncate().try_normalize().unwrap_or(DVec2::X);
        valve_symbol(d, v.point.truncate(), dir, mm, v.kind, Some(v.element));
    }
}

/// The plan symbol of a valve at `c` on a pipe running along `dir`: two
/// triangles meeting at their tips (a gate valve); a ball valve adds a
/// hollow circle, a globe valve a filled one, and a check valve fills the
/// triangle the flow leaves through. `mm` is one paper millimeter.
pub(crate) fn valve_symbol(
    d: &mut Drawing2D,
    c: DVec2,
    dir: DVec2,
    mm: f64,
    kind: cc_core::ValveKind,
    id: Option<cc_core::ElementId>,
) {
    use cc_core::ValveKind as K;
    let (l, h) = (dir * (2.0 * mm), dir.perp() * (1.4 * mm));
    let before = vec![c, c - l + h, c - l - h];
    let after = vec![c, c + l + h, c + l - h];
    for (i, tri) in [before, after].into_iter().enumerate() {
        let filled = kind == K::Check && i == 1;
        d.fill(tri.clone(), if filled { INK } else { [255, 255, 255, 255] }, id);
        d.polyline(&tri, true, LineStyle::Projection, id);
    }
    match kind {
        K::Ball => {
            let pts = circle(c, 0.7 * mm);
            d.fill(pts.clone(), [255, 255, 255, 255], id);
            d.polyline(&pts, true, LineStyle::Projection, id);
        }
        K::Globe => {
            let pts = circle(c, 0.7 * mm);
            d.fill(pts.clone(), INK, id);
            d.polyline(&pts, true, LineStyle::Projection, id);
        }
        K::Gate | K::Check => {}
    }
}

/// Tag text of a cable tray: its section (width × height, mm or inches) and
/// the circuits it carries.
fn tray_tag(project: &Project, e: &Element, width: f64, height: f64) -> String {
    let mm = |v: f64| format::size(v, format::MetricSize::MmNumber(0));
    let size = format!("{}×{}", mm(width), mm(height));
    let codes: Vec<&str> = e.systems().iter().filter_map(|s| project.system(*s)).map(|s| s.label()).collect();
    if codes.is_empty() { size } else { format!("{size} ({})", codes.join(", ")) }
}

/// A cable tray run in an electrical plan: its outline as wide as the tray,
/// lightly tinted in its circuit's color, with the rungs of a ladder tray
/// and its tag; a vertical tray shows its section crossed.
fn tray(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, e: &Element, s: &cc_core::MepSegment, mm: f64) {
    let Some(g) = geom.segments.get(&e.id) else { return };
    let Some(section) = g.tray else { return };
    let c = project.system_color(e);
    let tint = |v: u8| (255.0 - (255.0 - v as f64) * 0.25) as u8;
    let fill = [tint(c[0]), tint(c[1]), tint(c[2]), 255];
    let style = if s.start_offset.max(s.end_offset) < IN_FLOOR { LineStyle::Hidden } else { LineStyle::Projection };
    let text = tray_tag(project, e, section.width, section.height);
    let (w, h) = (section.width * 0.5, section.height * 0.5);
    if s.is_vertical() {
        let o = s.start;
        let pts = vec![o + DVec2::new(-w, -h), o + DVec2::new(w, -h), o + DVec2::new(w, h), o + DVec2::new(-w, h)];
        d.fill(pts.clone(), fill, Some(e.id));
        d.polyline(&pts, true, style, Some(e.id));
        d.line(pts[0], pts[2], LineStyle::Thin, Some(e.id));
        d.line(pts[1], pts[3], LineStyle::Thin, Some(e.id));
        d.tag_text(o + DVec2::new(w + mm, h + mm), text, 2.0, Anchor::LeftBottom, 0.0, Some(e.id));
        return;
    }
    let length = s.plan_length();
    let dir = (s.end - s.start) / length;
    let n = dir.perp() * w;
    let pts = vec![s.start - n, s.end - n, s.end + n, s.start + n];
    d.fill(pts.clone(), fill, Some(e.id));
    d.polyline(&pts, true, style, Some(e.id));
    // Rungs, when they stand at least 1.5 mm apart on paper.
    let spacing = cc_core::mep::TRAY_RUNG_SPACING;
    if section.style == cc_core::TrayStyle::Ladder && spacing >= 1.5 * mm {
        let count = ((length / spacing).floor() as usize).min(1000);
        let step = if count > 0 { length / count as f64 } else { 0.0 };
        for i in 0..count {
            let p = s.start + dir * (step * (i as f64 + 0.5));
            d.line(p - n, p + n, LineStyle::Thin, Some(e.id));
        }
    }
    if length < 15.0 * mm {
        return;
    }
    let (dir, angle) = {
        let a = dir.to_angle();
        if dir.x < -1e-9 || (dir.x.abs() <= 1e-9 && dir.y < 0.0) { (-dir, a + std::f64::consts::PI) } else { (dir, a) }
    };
    let mid = (s.start + s.end) * 0.5 + dir.perp() * (w + 0.8 * mm);
    d.tag_text(mid, text, 2.0, Anchor::CenterBottom, angle, Some(e.id));
}

/// A fitting in plan: a dot in the system's color on pipes; on conduits, a
/// box where three or more runs meet (elbows and couplings show as the runs).
fn fitting(d: &mut Drawing2D, project: &Project, f: &cc_geom::mep_net::Fitting, mm: f64) {
    use cc_geom::mep_net::FittingKind as K;
    let id = f.runs.first().copied();
    let c = DVec2::new(f.point.x, f.point.y);
    match (f.category, f.kind) {
        (cc_core::Category::Conduit, K::Tee | K::Cross) => {
            let h = 1.5 * mm;
            let square =
                vec![c + DVec2::new(-h, -h), c + DVec2::new(h, -h), c + DVec2::new(h, h), c + DVec2::new(-h, h)];
            d.fill(square.clone(), [255, 255, 255, 255], id);
            d.polyline(&square, true, LineStyle::Projection, id);
        }
        (cc_core::Category::Conduit, _) => {}
        _ => {
            let fallback = if f.category == cc_core::Category::LightningConductor {
                cc_core::SystemKind::Lightning.default_color()
            } else {
                [128, 128, 128]
            };
            let color = f.system.and_then(|s| project.system(s)).map_or(fallback, |s| s.color());
            let pts = circle(c, f.diameter().max(0.8 * mm) * 0.75);
            d.fill(pts.clone(), [color[0], color[1], color[2], 255], id);
            d.polyline(&pts, true, LineStyle::Thin, id);
        }
    }
}

/// A band of `width` from `a` to `b` in `color`, with a fine outline (dashed when hidden).
fn band(d: &mut Drawing2D, a: DVec2, b: DVec2, width: f64, color: [u8; 3], style: LineStyle, e: &Element) {
    let Some(dir) = (b - a).try_normalize() else { return };
    let n = dir.perp() * (width * 0.5);
    let pts = vec![a - n, b - n, b + n, a + n];
    d.fill(pts.clone(), [color[0], color[1], color[2], 255], Some(e.id));
    let outline = if style == LineStyle::Hidden { LineStyle::Hidden } else { LineStyle::Thin };
    d.polyline(&pts, true, outline, Some(e.id));
}

fn circle(c: DVec2, r: f64) -> Vec<DVec2> {
    (0..24).map(|i| c + DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU) * r).collect()
}

/// Tag text of a segment: its systems' codes and its diameter.
fn tag(project: &Project, e: &Element, diameter: f64) -> String {
    let codes: Vec<&str> = e.systems().iter().filter_map(|s| project.system(*s)).map(|s| s.label()).collect();
    let size = format!("Ø{}", format::size(diameter, format::MetricSize::MmNumber(0)));
    match e.kind {
        // Conduits carry circuit numbers; pipes their system code.
        ElementKind::Conduit(_) if !codes.is_empty() => format!("{size} ({})", codes.join(", ")),
        _ if !codes.is_empty() => format!("{} {size}", codes.join(", ")),
        _ => size,
    }
}

/// Run `e` with its elevations `s` above the plan's level.
fn segment(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    e: &Element,
    s: &cc_core::MepSegment,
    cut_height: f64,
    mm: f64,
) {
    let Some(g) = geom.segments.get(&e.id) else { return };
    let color = project.system_color(e);
    let conduit = matches!(e.kind, ElementKind::Conduit(_));
    // Pipes above the cut plane are dashed; conduits only when they run in the floor.
    let above = !conduit && s.start_offset.min(s.end_offset) > cut_height;
    let style = if above || (conduit && s.start_offset.max(s.end_offset) < IN_FLOOR) {
        LineStyle::Hidden
    } else {
        LineStyle::Projection
    };
    let text = tag(project, e, g.diameter);
    if s.is_vertical() {
        // A riser: a circle (at least 2 mm across) with its tag beside it.
        let r = (g.diameter * 0.5).max(mm);
        let pts = circle(s.start, r);
        d.fill(pts.clone(), [color[0], color[1], color[2], 255], Some(e.id));
        d.polyline(&pts, true, style, Some(e.id));
        d.tag_text(s.start + DVec2::new(r + mm, r + mm), text, 2.0, Anchor::LeftBottom, 0.0, Some(e.id));
        return;
    }
    // At least 0.8 mm on paper, so the system's color reads.
    let width = g.diameter.max(0.8 * mm);
    if conduit && style == LineStyle::Hidden {
        d.line(s.start, s.end, style, Some(e.id));
    } else {
        band(d, s.start, s.end, width, color, style, e);
    }
    // Tags along runs at least 15 mm long on paper, read from below or the right.
    if s.plan_length() < 15.0 * mm {
        return;
    }
    let dir = (s.end - s.start) / s.plan_length();
    let (dir, angle) = {
        let a = dir.to_angle();
        if dir.x < -1e-9 || (dir.x.abs() <= 1e-9 && dir.y < 0.0) { (-dir, a + std::f64::consts::PI) } else { (dir, a) }
    };
    let mid = (s.start + s.end) * 0.5 + dir.perp() * (width * 0.5 + 0.8 * mm);
    let slope = s.slope();
    let text = if matches!(e.kind, ElementKind::Pipe(_)) && slope.abs() > 1e-4 {
        format!("{text}  i = {}", format::slope(slope.abs()))
    } else {
        text
    };
    d.tag_text(mid, text, 2.0, Anchor::CenterBottom, angle, Some(e.id));
}

/// Where a symbol is drawn: the device's insertion point and rotation.
/// Wall devices stand out towards their local -Y.
pub(crate) struct Frame {
    pub origin: DVec2,
    pub angle: f64,
    /// One paper millimeter in model meters.
    pub mm: f64,
}

impl Frame {
    /// A point `x`, `y` paper millimeters from the origin in the device's frame.
    fn at(&self, x: f64, y: f64) -> DVec2 {
        self.origin + DVec2::from_angle(self.angle).rotate(DVec2::new(x, y) * self.mm)
    }
}

/// The plan symbol of an electrical device mounted `height` above its level.
pub(crate) fn symbol(
    d: &mut Drawing2D,
    f: &Frame,
    device: DeviceKind,
    height: f64,
    power: f64,
    circuit: Option<&str>,
    id: Option<cc_core::ElementId>,
) {
    let style = LineStyle::Projection;
    let label = |d: &mut Drawing2D, at: DVec2, text: &str| d.tag_text(at, text, 2.0, Anchor::Center, 0.0, id);
    match device {
        DeviceKind::Outlet | DeviceKind::DedicatedOutlet => {
            // Base on the wall, apex into the room.
            let (l, r, apex) = (f.at(-1.75, 0.0), f.at(1.75, 0.0), f.at(0.0, -3.0));
            let tri = vec![l, r, apex];
            if height >= MEDIUM || device == DeviceKind::DedicatedOutlet {
                d.fill(tri.clone(), INK, id);
            } else if height >= LOW {
                d.fill(vec![l, f.at(0.0, 0.0), apex], INK, id);
            }
            d.polyline(&tri, true, style, id);
            let text = match (device, circuit) {
                (DeviceKind::DedicatedOutlet, Some(c)) => format!("{c} · {} VA", format::number(power, 0)),
                (DeviceKind::DedicatedOutlet, None) => format!("{} VA", format::number(power, 0)),
                (_, Some(c)) => c.to_string(),
                (_, None) => String::new(),
            };
            if !text.is_empty() {
                label(d, f.at(0.0, -5.0), &text);
            }
        }
        DeviceKind::Switch => {
            let pts = circle(f.at(0.0, -1.5), 1.0 * f.mm);
            d.fill(pts.clone(), INK, id);
            d.polyline(&pts, true, style, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -4.0), c);
            }
        }
        DeviceKind::CeilingLight => {
            let r = 3.0;
            d.polyline(&circle(f.origin, r * f.mm), true, style, id);
            d.line(f.at(-r, 0.0), f.at(r, 0.0), LineStyle::Thin, id);
            if let Some(c) = circuit {
                label(d, f.origin + DVec2::new(0.0, 1.4 * f.mm), c);
            }
            label(d, f.origin - DVec2::new(0.0, 1.4 * f.mm), &format::number(power, 0));
        }
        DeviceKind::WallLight => {
            // A half circle against the wall.
            let r = 2.5;
            let pts: Vec<DVec2> = (0..=12)
                .map(|i| {
                    let t = std::f64::consts::PI * (1.0 + i as f64 / 12.0);
                    f.at(r * t.cos(), r * t.sin())
                })
                .collect();
            d.polyline(&pts, true, style, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -4.5), c);
            }
        }
        DeviceKind::Panel => {
            let (w, h) = (3.0, 2.5);
            let rect = vec![f.at(-w, 0.0), f.at(w, 0.0), f.at(w, -h), f.at(-w, -h)];
            d.fill(vec![f.at(-w, 0.0), f.at(w, 0.0), f.at(-w, -h)], INK, id);
            d.polyline(&rect, true, LineStyle::Cut, id);
        }
        DeviceKind::DataOutlet => {
            // A hollow triangle on the wall with a bar across its apex.
            let tri = vec![f.at(-1.75, 0.0), f.at(1.75, 0.0), f.at(0.0, -3.0)];
            d.fill(tri.clone(), [255, 255, 255, 255], id);
            d.polyline(&tri, true, style, id);
            d.line(f.at(-1.75, -3.0), f.at(1.75, -3.0), style, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -5.0), c);
            }
        }
        DeviceKind::Rack => {
            // A box with its diagonals, the codes of the cabling it serves beside it.
            let (w, h) = (3.0, 2.5);
            let rect = vec![f.at(-w, 0.0), f.at(w, 0.0), f.at(w, -h), f.at(-w, -h)];
            d.fill(rect.clone(), [255, 255, 255, 255], id);
            d.line(rect[0], rect[2], LineStyle::Thin, id);
            d.line(rect[1], rect[3], LineStyle::Thin, id);
            d.polyline(&rect, true, LineStyle::Cut, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -h - 2.0), c);
            }
        }
        DeviceKind::AirTerminal => {
            // A ring around a filled dot.
            d.polyline(&circle(f.origin, 2.0 * f.mm), true, style, id);
            let dot = circle(f.origin, 0.8 * f.mm);
            d.fill(dot.clone(), INK, id);
            d.polyline(&dot, true, style, id);
        }
        DeviceKind::EarthElectrode => {
            // A ring with the earthing sign: three bars, shorter downwards.
            let ring = circle(f.origin, 2.2 * f.mm);
            d.fill(ring.clone(), [255, 255, 255, 255], id);
            d.polyline(&ring, true, style, id);
            let o = f.origin;
            d.line(o + DVec2::new(0.0, 1.2) * f.mm, o + DVec2::new(0.0, 0.4) * f.mm, style, id);
            for (i, half) in [1.4, 0.9, 0.4].into_iter().enumerate() {
                let y = (0.4 - 0.6 * i as f64) * f.mm;
                d.line(o + DVec2::new(-half * f.mm, y), o + DVec2::new(half * f.mm, y), style, id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prim;
    use cc_core::{Category, Document, ElementId, MepSegment, SystemKind, TypeData, ViewKind};

    fn system(p: &Project, kind: SystemKind) -> cc_core::SystemId {
        *p.systems.iter().find(|(_, s)| s.kind == kind).unwrap().0
    }

    #[test]
    fn plumbing_plan_draws_pipes_with_tags_and_hides_them_elsewhere() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let sewer = system(&doc.project, SystemKind::Sanitary);
        let pipe_t = doc
            .project
            .types_of(Category::Pipe)
            .find(|t| matches!(t.data, TypeData::Pipe { diameter, .. } if (diameter - 0.1).abs() < 1e-9))
            .unwrap()
            .id;
        let (pipe, riser) = doc.edit("t", |e| {
            let mut run = MepSegment::new(DVec2::ZERO, DVec2::new(5.0, 0.0), -0.3);
            run.end_offset = -0.35;
            run.systems = vec![sewer];
            let mut up = MepSegment::new(DVec2::new(5.0, 0.0), DVec2::new(5.0, 0.0), -0.35);
            up.end_offset = 3.0;
            up.systems = vec![sewer];
            let add = |e: &mut cc_core::EditCtx, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(cc_core::Element {
                    id,
                    level,
                    type_id: pipe_t,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                    design_option: None,
                    paints: Vec::new(),
                });
                id
            };
            (add(e, ElementKind::Pipe(run)), add(e, ElementKind::Pipe(up)))
        });
        let geom = ModelGeometry::build(&doc.project);
        // The run and the elbow where the riser leaves it.
        assert_eq!(geom.solids_of(pipe).count(), 2);
        let plan = |mep| {
            cc_core::View::new(
                cc_core::ViewId(999),
                "",
                50,
                ViewKind::FloorPlan {
                    level,
                    cut_height: 1.2,
                    structural: false,
                    structure_tags: false,
                    mep,
                    underlay: Default::default(),
                },
            )
        };
        let d = crate::generate(&doc.project, &geom, &plan(Some(MepDiscipline::Plumbing))).unwrap();
        let texts: Vec<&str> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("ES Ø100") && t.contains("i = 1")), "{texts:?}");
        assert!(texts.contains(&"ES Ø100"), "{texts:?}");
        assert_eq!(d.hit_test(DVec2::new(2.5, 0.0), 0.01), Some(pipe));
        assert!(d.hit_test(DVec2::new(5.0, 0.0), 0.01).is_some());
        let colored = SystemKind::Sanitary.default_color();
        assert!(d.prims.iter().any(|p| matches!(p, Prim::Fill { color, element: Some(id), .. }
            if *id == riser && color[..3] == colored)));
        // Architectural and electrical plans leave pipes out.
        for mep in [None, Some(MepDiscipline::Electrical)] {
            let d = crate::generate(&doc.project, &geom, &plan(mep)).unwrap();
            assert!(d.prims.iter().all(|p| p.element() != Some(pipe)));
        }
        // The riser reaches the first floor: its plan shows it, without the ground floor's run.
        let first = doc.project.levels[1].id;
        let upstairs = cc_core::View {
            kind: ViewKind::FloorPlan {
                level: first,
                cut_height: 1.2,
                structural: false,
                structure_tags: false,
                mep: Some(MepDiscipline::Plumbing),
                underlay: Default::default(),
            },
            ..plan(None)
        };
        let d = crate::generate(&doc.project, &geom, &upstairs).unwrap();
        assert_eq!(d.hit_test(DVec2::new(5.0, 0.0), 0.01), Some(riser));
        assert!(d.prims.iter().all(|p| p.element() != Some(pipe)));
    }

    #[test]
    fn outlet_symbol_fills_with_height() {
        let fills = |height: f64| {
            let mut d = Drawing2D::default();
            let f = Frame { origin: DVec2::ZERO, angle: 0.0, mm: 0.05 };
            symbol(&mut d, &f, DeviceKind::Outlet, height, 100.0, Some("3"), None);
            let texts = d.prims.iter().filter(|p| matches!(p, Prim::Text { text, .. } if text == "3")).count();
            assert_eq!(texts, 1);
            d.prims.iter().filter(|p| matches!(p, Prim::Fill { .. })).count()
        };
        assert_eq!(fills(0.3), 0);
        assert_eq!(fills(1.1), 1);
        assert_eq!(fills(2.0), 1);
    }

    #[test]
    fn trays_show_in_electrical_plans_and_valves_in_plumbing_plans() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let (tray_t, valve_t) = doc.edit("t", |e| {
            let mut add = |data| {
                let id = cc_core::ElementTypeId(e.alloc_id());
                e.add_type(cc_core::ElementType {
                    id,
                    name: "t".into(),
                    data,
                    info: Default::default(),
                    uid: Default::default(),
                })
            };
            let tray = TypeData::CableTray {
                width: 0.3,
                height: 0.1,
                style: cc_core::TrayStyle::Ladder,
                material: String::new(),
            };
            (add(tray), add(TypeData::Valve { kind: cc_core::ValveKind::Check, material: String::new() }))
        });
        let pipe_t = doc.project.default_type(Category::Pipe).unwrap();
        let (tray, pipe, valve) = doc.edit("t", |e| {
            let mut add = |type_id, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(cc_core::Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                    design_option: None,
                    paints: Vec::new(),
                })
            };
            let tray = add(tray_t, ElementKind::CableTray(MepSegment::new(DVec2::ZERO, DVec2::new(6.0, 0.0), 2.6)));
            let pipe = add(pipe_t, ElementKind::Pipe(MepSegment::new(DVec2::new(0.0, 3.0), DVec2::new(4.0, 3.0), 0.6)));
            let valve = add(valve_t, ElementKind::Valve(cc_core::Valve { host: pipe, at: 2.0 }));
            (tray, pipe, valve)
        });
        let geom = ModelGeometry::build(&doc.project);
        let plan = |mep| {
            cc_core::View::new(
                cc_core::ViewId(999),
                "",
                50,
                ViewKind::FloorPlan {
                    level,
                    cut_height: 1.2,
                    structural: false,
                    structure_tags: false,
                    mep: Some(mep),
                    underlay: Default::default(),
                },
            )
        };
        let electrical = crate::generate(&doc.project, &geom, &plan(MepDiscipline::Electrical)).unwrap();
        // The tray is as wide as its type: picked 14 cm off its center line, tagged with its section.
        assert_eq!(electrical.hit_test(DVec2::new(3.0, 0.14), 0.001), Some(tray));
        assert!(electrical.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == "300×100")));
        assert!(electrical.prims.iter().all(|p| p.element() != Some(valve) && p.element() != Some(pipe)));
        let plumbing = crate::generate(&doc.project, &geom, &plan(MepDiscipline::Plumbing)).unwrap();
        assert_eq!(plumbing.hit_test(DVec2::new(2.0, 3.0), 0.001), Some(valve));
        assert!(plumbing.prims.iter().all(|p| p.element() != Some(tray)));
    }

    #[test]
    fn lightning_protection_and_cabling_have_plans_of_their_own() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let name = |key: &str| key.to_string().replace('-', "_");
        let (conductor_t, rod_t, outlet_t) = doc.edit("t", |e| {
            let mut add = |data| {
                let id = cc_core::ElementTypeId(e.alloc_id());
                e.add_type(cc_core::ElementType {
                    id,
                    name: "t".into(),
                    data,
                    info: Default::default(),
                    uid: Default::default(),
                })
            };
            let device = |d| {
                let (component, _, def) = cc_core::component::device_library(d, &name).unwrap();
                TypeData::Component { component, def }
            };
            (
                add(TypeData::LightningConductor { diameter: 0.008, material: String::new() }),
                add(device(DeviceKind::EarthElectrode)),
                add(device(DeviceKind::DataOutlet)),
            )
        });
        let (ring, rod, outlet) = doc.edit("t", |e| {
            let mut add = |type_id, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(cc_core::Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                    design_option: None,
                    paints: Vec::new(),
                })
            };
            let run = MepSegment::new(DVec2::ZERO, DVec2::new(6.0, 0.0), -0.5);
            let ring = cc_core::LightningConductor { run, role: ConductorRole::Earth };
            let ring = add(conductor_t, ElementKind::LightningConductor(ring));
            let at = |x: f64, y: f64| cc_core::Component { position: DVec2::new(x, y), ..Default::default() };
            let rod = add(rod_t, ElementKind::Component(at(0.0, 3.0)));
            let outlet = add(outlet_t, ElementKind::Component(at(4.0, 3.0)));
            (ring, rod, outlet)
        });
        let geom = ModelGeometry::build(&doc.project);
        let plan = |mep| {
            let kind = ViewKind::FloorPlan {
                level,
                cut_height: 1.2,
                structural: false,
                structure_tags: false,
                mep: Some(mep),
                underlay: Default::default(),
            };
            cc_core::View::new(cc_core::ViewId(999), "", 50, kind)
        };
        let has = |d: &Drawing2D, id| d.prims.iter().any(|p| p.element() == Some(id));
        let lightning = crate::generate(&doc.project, &geom, &plan(MepDiscipline::Lightning)).unwrap();
        assert!(has(&lightning, ring) && has(&lightning, rod) && !has(&lightning, outlet));
        let communications = crate::generate(&doc.project, &geom, &plan(MepDiscipline::Communications)).unwrap();
        assert!(has(&communications, outlet) && !has(&communications, ring) && !has(&communications, rod));
        let electrical = crate::generate(&doc.project, &geom, &plan(MepDiscipline::Electrical)).unwrap();
        assert!(!has(&electrical, outlet) && !has(&electrical, ring) && !has(&electrical, rod));
    }
}
