//! Turning raw entities into flat primitives: object coordinate systems,
//! block insertion (nested, arrayed, cycle-safe and bounded), colors and
//! layers by block, and curve tessellation where a primitive cannot stay
//! an arc.

use crate::Limits;
use crate::dxf::{Entity, rgb};
use crate::geom::{self, Affine2};
use crate::text;
use crate::{CadBlock, CadInsert};
use cc_core::cad::{CadHAlign, CadLayer, CadPrim, CadVAlign, MAX_POLY_POINTS, aci_color};
use glam::{DAffine3, DVec2, DVec3};
use std::collections::HashMap;
use std::f64::consts::TAU;

/// A block definition: its base point and entities.
#[derive(Debug, Clone, Default)]
pub struct Block {
    pub base: DVec3,
    pub entities: Vec<Entity>,
}

/// Where an entity is drawn: block to world transform, the layer that
/// replaces layer "0" and the color that replaces BYBLOCK.
#[derive(Clone)]
struct Ctx {
    t: DAffine3,
    layer0: Option<u32>,
    byblock: Option<[u8; 3]>,
    depth: usize,
    /// Blocks being inserted (cycle guard).
    stack: Vec<String>,
}

pub struct Flattener {
    limits: Limits,
    layers: Vec<CadLayer>,
    index: HashMap<String, u32>,
    out: Vec<CadPrim>,
    /// Primitives emitted in all (the drawing's and the kept blocks').
    emitted: usize,
    work: usize,
    /// Points and text characters handled ([`Limits::max_points`]).
    points: usize,
    stopped: bool,
    /// Where block references go when blocks are kept ([`Limits::keep_blocks`]):
    /// the drawing's, then each kept block's in turn.
    refs: Vec<CadInsert>,
    /// Blocks referenced and not yet kept.
    wanted: Vec<String>,
    /// Block references recorded in all.
    kept_refs: usize,
}

impl Flattener {
    pub fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.clone(),
            layers: Vec::new(),
            index: HashMap::new(),
            out: Vec::new(),
            emitted: 0,
            work: 0,
            points: 0,
            stopped: false,
            refs: Vec::new(),
            wanted: Vec::new(),
            kept_refs: 0,
        }
    }

    /// The block references of the drawing's entities (blocks kept).
    pub fn take_inserts(&mut self) -> Vec<CadInsert> {
        std::mem::take(&mut self.refs)
    }

    /// The blocks the drawing references, each read on its own (base point
    /// at the origin, layer 0 and BYBLOCK left as they are), with the
    /// references they make to others; a reference that would close a
    /// cycle is dropped. Call after the entities and [`Self::take_inserts`].
    pub fn kept_blocks(&mut self, blocks: &HashMap<String, Block>) -> Vec<CadBlock> {
        let mut out: Vec<CadBlock> = Vec::new();
        let mut done = std::collections::HashSet::new();
        let saved = std::mem::take(&mut self.out);
        let mut queue = std::mem::take(&mut self.wanted);
        while let Some(name) = queue.pop() {
            if self.stopped || !done.insert(name.clone()) {
                continue;
            }
            let Some(block) = blocks.get(&name) else { continue };
            let ctx = Ctx {
                t: DAffine3::from_translation(-block.base),
                layer0: None,
                byblock: None,
                depth: 1,
                stack: vec![name.clone()],
            };
            for e in &block.entities {
                if self.stopped {
                    break;
                }
                self.entity(e, blocks, &ctx);
            }
            queue.append(&mut self.wanted);
            out.push(CadBlock { name, prims: std::mem::take(&mut self.out), inserts: std::mem::take(&mut self.refs) });
        }
        self.out = saved;
        drop_cycles(&mut out);
        out
    }

    /// Register the layers of the LAYER table (kept in file order).
    pub fn set_layers(&mut self, layers: &[CadLayer]) {
        for l in layers {
            self.layer_index(&l.name, Some(l));
        }
    }

    fn layer_index(&mut self, name: &str, def: Option<&CadLayer>) -> u32 {
        let key = name.to_ascii_uppercase();
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.layers.len() as u32;
        let layer = def.cloned().unwrap_or(CadLayer { name: name.to_string(), color: [0, 0, 0], visible: true });
        self.layers.push(layer);
        self.index.insert(key, i);
        i
    }

    /// The primitives, the layers and whether a limit cut the drawing short.
    pub fn finish(mut self) -> (Vec<CadPrim>, Vec<CadLayer>, bool) {
        if self.layers.is_empty() {
            self.layer_index("0", None);
        }
        (self.out, self.layers, self.stopped)
    }

    /// Flatten a model space entity.
    pub fn top_level(&mut self, e: &Entity, blocks: &HashMap<String, Block>) {
        let ctx = Ctx { t: DAffine3::IDENTITY, layer0: None, byblock: None, depth: 0, stack: Vec::new() };
        self.entity(e, blocks, &ctx);
    }

    /// Count `n` points or characters against [`Limits::max_points`];
    /// false (and the reader stopped) once the budget is spent.
    fn charge(&mut self, n: usize) -> bool {
        self.points = self.points.saturating_add(n);
        if self.points > self.limits.max_points {
            self.stopped = true;
        }
        !self.stopped
    }

    fn emit(&mut self, prim: CadPrim) {
        if self.emitted >= self.limits.max_prims {
            self.stopped = true;
            return;
        }
        let size = match &prim {
            CadPrim::Polyline { pts, .. } | CadPrim::Fill { pts, .. } => pts.len(),
            CadPrim::Text { text, .. } => text.len(),
            _ => 1,
        };
        if !self.charge(size) {
            return;
        }
        if prim.is_valid() {
            self.emitted += 1;
            self.out.push(prim);
        }
    }

    fn layer(&mut self, e: &Entity, ctx: &Ctx) -> u32 {
        let name = e.str(8);
        let name = if name.is_empty() { "0" } else { name };
        match ctx.layer0 {
            Some(l) if name == "0" => l,
            _ => self.layer_index(name, None),
        }
    }

    fn color(&self, e: &Entity, ctx: &Ctx) -> Option<[u8; 3]> {
        if let Some(v) = e.get(420) {
            return Some(rgb(v.int()));
        }
        match e.int(62, 256) {
            0 => ctx.byblock,
            256 => None,
            c => Some(aci_color(c)),
        }
    }

    fn entity(&mut self, e: &Entity, blocks: &HashMap<String, Block>, ctx: &Ctx) {
        if self.stopped {
            return;
        }
        self.work += 1;
        if self.work > self.limits.max_work {
            self.stopped = true;
            return;
        }
        // Reading an entity costs its size, each time a block inserts it.
        if !self.charge(e.groups.len().saturating_add(e.children.len())) {
            return;
        }
        // Invisible entities.
        if e.int(60, 0) == 1 {
            return;
        }
        let layer = self.layer(e, ctx);
        let color = self.color(e, ctx);
        match e.name.as_str() {
            "LINE" => {
                let pts = [e.point(10), e.point(11)];
                self.points3(&ctx.t, &pts, false, layer, color);
            }
            "POINT" => {
                // A snap target, too small to draw.
                let p = xy(ctx.t.transform_point3(e.point(10)));
                self.emit(CadPrim::Polyline { layer, color, closed: false, pts: vec![p, p], bulges: Vec::new() });
            }
            "CIRCLE" | "ARC" => {
                let n = e.normal();
                let c = e.point(10);
                let a = Affine2::of_plane(&(ctx.t * geom::ocs(n)), c.z);
                let r = e.real(40, 0.0);
                let (start, sweep) = if e.name == "CIRCLE" {
                    (0.0, TAU)
                } else {
                    let s = e.real(50, 0.0).to_radians();
                    let mut sw = (e.real(51, 0.0).to_radians() - s).rem_euclid(TAU);
                    if sw < 1e-12 {
                        sw = TAU;
                    }
                    (s, sw)
                };
                self.arc(&a, DVec2::new(c.x, c.y), r, start, sweep, layer, color);
            }
            "ELLIPSE" => {
                let n = e.normal();
                let pts = geom::ellipse_points(
                    e.point(10),
                    e.point(11),
                    e.real(40, 1.0),
                    n,
                    e.real(41, 0.0),
                    e.real(42, TAU),
                );
                let closed = (e.real(42, TAU) - e.real(41, 0.0)).rem_euclid(TAU) < 1e-9;
                self.points3(&ctx.t, &pts, closed && pts.len() > 2, layer, color);
            }
            "LWPOLYLINE" => self.lwpolyline(e, ctx, layer, color),
            "POLYLINE" => self.polyline(e, ctx, layer, color),
            "SPLINE" => self.spline(e, ctx, layer, color),
            "TEXT" | "ATTRIB" => self.text(e, ctx, layer, color),
            "MTEXT" => self.mtext(e, ctx, layer, color),
            "SOLID" | "TRACE" => {
                let n = e.normal();
                let corners = [e.point(10), e.point(11), e.point(13), e.point(12)];
                let a = Affine2::of_plane(&(ctx.t * geom::ocs(n)), corners[0].z);
                let mut pts: Vec<DVec2> = corners.iter().map(|p| a.apply(DVec2::new(p.x, p.y))).collect();
                pts.dedup_by(|x, y| x.distance(*y) < 1e-12);
                if pts.len() >= 3 {
                    self.emit(CadPrim::Fill { layer, color, pts: pts.clone() });
                    self.emit(CadPrim::Polyline { layer, color, closed: true, pts, bulges: Vec::new() });
                }
            }
            "3DFACE" => {
                let mut pts = vec![e.point(10), e.point(11), e.point(12), e.point(13)];
                pts.dedup_by(|x, y| x.distance(*y) < 1e-12);
                self.points3(&ctx.t, &pts, true, layer, color);
            }
            "LEADER" => {
                let pts = e.points(10);
                self.points3(&ctx.t, &pts, false, layer, color);
            }
            "HATCH" => self.hatch(e, ctx, layer, color),
            "INSERT" => self.insert(e, blocks, ctx, layer, color),
            "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => {
                let name = e.str(2).to_ascii_uppercase();
                let t = ctx.t * geom::ocs(e.normal()) * DAffine3::from_translation(e.point(12));
                let block_color = color.or_else(|| self.layers.get(layer as usize).map(|l| l.color));
                self.block(&name, blocks, ctx, t, layer, block_color);
            }
            _ => {}
        }
    }

    /// 3D points of a world-coordinate entity, projected on the plan.
    fn points3(&mut self, t: &DAffine3, pts: &[DVec3], closed: bool, layer: u32, color: Option<[u8; 3]>) {
        let pts: Vec<DVec2> = pts.iter().take(MAX_POLY_POINTS).map(|p| xy(t.transform_point3(*p))).collect();
        if pts.len() >= 2 {
            self.emit(CadPrim::Polyline { layer, color, closed, pts, bulges: Vec::new() });
        }
    }

    /// An arc of a plane mapped by `a`: still an arc under a similarity,
    /// else (a stretched block, a tilted plane) tessellated.
    #[allow(clippy::too_many_arguments)]
    fn arc(&mut self, a: &Affine2, c: DVec2, r: f64, start: f64, sweep: f64, layer: u32, color: Option<[u8; 3]>) {
        if !(r.is_finite() && r > 0.0) {
            return;
        }
        match a.similarity() {
            Some(s) => {
                let dir = a.vector(DVec2::from_angle(start));
                let sign = a.det().signum();
                self.emit(CadPrim::Arc {
                    layer,
                    color,
                    center: a.apply(c),
                    radius: r * s,
                    start: dir.y.atan2(dir.x),
                    sweep: sweep * sign,
                });
            }
            None => {
                let pts: Vec<DVec2> = geom::arc_points(c, r, start, sweep).into_iter().map(|p| a.apply(p)).collect();
                let closed = sweep.abs() >= TAU - 1e-9;
                self.emit(CadPrim::Polyline { layer, color, closed, pts, bulges: Vec::new() });
            }
        }
    }

    /// A 2D polyline of a plane mapped by `a`, keeping its bulges under a similarity.
    #[allow(clippy::too_many_arguments)]
    fn poly2(
        &mut self,
        a: &Affine2,
        pts: Vec<DVec2>,
        mut bulges: Vec<f64>,
        closed: bool,
        layer: u32,
        color: Option<[u8; 3]>,
    ) {
        if pts.len() < 2 {
            return;
        }
        if bulges.iter().all(|b| b.abs() < 1e-9) {
            bulges.clear();
        }
        if bulges.is_empty() || a.similarity().is_some() {
            let sign = a.det().signum();
            let pts = pts.iter().map(|p| a.apply(*p)).collect();
            bulges.iter_mut().for_each(|b| *b *= sign);
            self.emit(CadPrim::Polyline { layer, color, closed, pts, bulges });
            return;
        }
        // Tessellate the bulged segments, then map.
        let n = pts.len();
        let segs = if closed { n } else { n - 1 };
        let mut out = Vec::new();
        for i in 0..segs {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            match geom::bulge_arc(p, q, bulges.get(i).copied().unwrap_or(0.0)) {
                Some((c, r, s, sw)) => {
                    let arc = geom::arc_points(c, r, s, sw);
                    out.extend(arc.iter().take(arc.len().saturating_sub(1)));
                }
                None => out.push(p),
            }
            if out.len() >= MAX_POLY_POINTS {
                break;
            }
        }
        if !closed {
            out.push(pts[n - 1]);
        }
        out.truncate(MAX_POLY_POINTS);
        let pts = out.into_iter().map(|p| a.apply(p)).collect();
        self.emit(CadPrim::Polyline { layer, color, closed, pts, bulges: Vec::new() });
    }

    fn lwpolyline(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let mut pts: Vec<DVec2> = Vec::new();
        let mut bulges: Vec<f64> = Vec::new();
        for (c, v) in &e.groups {
            match c {
                10 => {
                    if pts.len() >= MAX_POLY_POINTS {
                        break;
                    }
                    pts.push(DVec2::new(finite(v.real()), 0.0));
                    bulges.push(0.0);
                }
                20 => {
                    if let Some(p) = pts.last_mut() {
                        p.y = finite(v.real());
                    }
                }
                42 => {
                    if let Some(b) = bulges.last_mut() {
                        *b = finite(v.real());
                    }
                }
                _ => {}
            }
        }
        let closed = e.int(70, 0) & 1 == 1;
        let a = Affine2::of_plane(&(ctx.t * geom::ocs(e.normal())), e.real(38, 0.0));
        self.poly2(&a, pts, bulges, closed, layer, color);
    }

    fn polyline(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let flags = e.int(70, 0);
        let closed = flags & 1 == 1;
        let vertex = |v: &Entity| v.point(10);
        if flags & 64 != 0 {
            // Polyface mesh: positions, then faces by 1-based index (negative: hidden edge).
            let positions: Vec<DVec3> = e.children.iter().filter(|v| v.int(70, 0) & 64 != 0).map(vertex).collect();
            for face in e.children.iter().filter(|v| v.int(70, 0) & 192 == 128) {
                let idx: Vec<i64> = [71, 72, 73, 74].iter().map(|c| face.int(*c, 0)).filter(|i| *i != 0).collect();
                for k in 0..idx.len() {
                    let (i, j) = (idx[k], idx[(k + 1) % idx.len()]);
                    if i < 0 {
                        continue;
                    }
                    let get = |i: i64| positions.get((i.unsigned_abs() as usize).wrapping_sub(1)).copied();
                    if let (Some(p), Some(q)) = (get(i), get(j)) {
                        self.points3(&ctx.t, &[p, q], false, layer, color);
                    }
                    if self.stopped {
                        return;
                    }
                }
            }
            return;
        }
        if flags & 16 != 0 {
            // Polygon mesh: an M × N grid of vertices.
            let (m, n) = (e.int(71, 0).max(0) as usize, e.int(72, 0).max(0) as usize);
            let pts: Vec<DVec3> = e.children.iter().map(vertex).collect();
            if m == 0 || n == 0 || m.saturating_mul(n) > pts.len() {
                return;
            }
            for i in 0..m {
                let row: Vec<DVec3> = (0..n).filter_map(|j| pts.get(i * n + j).copied()).collect();
                self.points3(&ctx.t, &row, flags & 32 != 0, layer, color);
            }
            for j in 0..n {
                let col: Vec<DVec3> = (0..m).filter_map(|i| pts.get(i * n + j).copied()).collect();
                self.points3(&ctx.t, &col, closed, layer, color);
            }
            return;
        }
        // Spline-fit polylines keep their fitted vertices, not the frame.
        let fitted = flags & 4 != 0;
        let verts: Vec<&Entity> =
            e.children.iter().filter(|v| v.name == "VERTEX" && !(fitted && v.int(70, 0) & 16 != 0)).collect();
        if flags & 8 != 0 {
            let pts: Vec<DVec3> = verts.iter().map(|v| vertex(v)).collect();
            self.points3(&ctx.t, &pts, closed, layer, color);
            return;
        }
        let pts: Vec<DVec2> = verts.iter().map(|v| v.point(10)).map(|p| DVec2::new(p.x, p.y)).collect();
        let bulges: Vec<f64> = verts.iter().map(|v| v.real(42, 0.0)).collect();
        let a = Affine2::of_plane(&(ctx.t * geom::ocs(e.normal())), e.point(10).z);
        self.poly2(&a, pts, bulges, closed, layer, color);
    }

    fn spline(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let degree = e.int(71, 3).clamp(0, 11) as usize;
        let knots = e.reals(40);
        let weights = e.reals(41);
        let control = e.points(10);
        let closed = e.int(70, 0) & 1 == 1;
        let pts = geom::spline_points(degree, &knots, &control, &weights).or_else(|| {
            let fit = e.points(11);
            (fit.len() >= 2).then_some(fit)
        });
        if let Some(pts) = pts {
            self.points3(&ctx.t, &pts, closed && pts.len() > 2, layer, color);
        }
    }

    fn text(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        if e.name == "ATTRIB" && e.int(70, 0) & 1 == 1 {
            return;
        }
        if !self.charge(e.str(1).len()) {
            return;
        }
        let s = text::plain(e.str(1));
        let height = e.real(40, 0.0);
        if s.trim().is_empty() || !(height.is_finite() && height > 0.0) {
            return;
        }
        let hj = e.int(72, 0);
        let vj = e.int(if e.name == "ATTRIB" { 74 } else { 73 }, 0);
        let (p10, p11) = (e.point(10), e.point(11));
        let rot = e.real(50, 0.0).to_radians();
        let (pos, dir, h, v) = match (hj, vj) {
            (0, 0) => (p10, DVec2::from_angle(rot), CadHAlign::Left, CadVAlign::Baseline),
            (3 | 5, _) => {
                let d = DVec2::new(p11.x - p10.x, p11.y - p10.y);
                (p10, d.try_normalize().unwrap_or(DVec2::from_angle(rot)), CadHAlign::Left, CadVAlign::Baseline)
            }
            (4, _) => (p11, DVec2::from_angle(rot), CadHAlign::Center, CadVAlign::Middle),
            _ => {
                let h = match hj {
                    1 => CadHAlign::Center,
                    2 => CadHAlign::Right,
                    _ => CadHAlign::Left,
                };
                let v = match vj {
                    1 => CadVAlign::Bottom,
                    2 => CadVAlign::Middle,
                    3 => CadVAlign::Top,
                    _ => CadVAlign::Baseline,
                };
                (p11, DVec2::from_angle(rot), h, v)
            }
        };
        let a = Affine2::of_plane(&(ctx.t * geom::ocs(e.normal())), p10.z);
        self.place_text(&a, DVec2::new(pos.x, pos.y), dir, height, s, h, v, layer, color);
    }

    /// A text at `pos` along `dir` in a plane mapped by `a`.
    #[allow(clippy::too_many_arguments)]
    fn place_text(
        &mut self,
        a: &Affine2,
        pos: DVec2,
        dir: DVec2,
        height: f64,
        text: String,
        h: CadHAlign,
        v: CadVAlign,
        layer: u32,
        color: Option<[u8; 3]>,
    ) {
        let d = a.vector(dir);
        let len = d.length();
        if len < 1e-15 {
            return;
        }
        // Height across the baseline: |det| / |a·dir| for a unit `dir`.
        let height = height * a.det().abs() / len;
        let text: String = text.chars().take(cc_core::cad::MAX_TEXT_CHARS).collect();
        self.emit(CadPrim::Text { layer, color, pos: a.apply(pos), height, rotation: d.y.atan2(d.x), text, h, v });
    }

    fn mtext(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let height = e.real(40, 0.0);
        if !(height.is_finite() && height > 0.0) {
            return;
        }
        let mut raw = String::new();
        for (c, v) in &e.groups {
            if *c == 3 {
                raw.push_str(v.str());
            }
        }
        raw.push_str(e.str(1));
        if !self.charge(raw.len()) {
            return;
        }
        let lines: Vec<String> = text::mtext_lines(&raw).into_iter().take(1000).collect();
        let n = e.normal();
        let (ax, ay, _) = geom::ocs_axes(n);
        let xdir = if e.has(11) {
            e.point(11).try_normalize().unwrap_or(ax)
        } else {
            let r = e.real(50, 0.0);
            ax * r.cos() + ay * r.sin()
        };
        let up = n.cross(xdir).try_normalize().unwrap_or(DVec3::Y);
        let attach = e.int(71, 1).clamp(1, 9);
        let h = match (attach - 1) % 3 {
            1 => CadHAlign::Center,
            2 => CadHAlign::Right,
            _ => CadHAlign::Left,
        };
        let factor = e.real(44, 1.0);
        let spacing = height * 5.0 / 3.0 * if (0.25..=4.0).contains(&factor) { factor } else { 1.0 };
        let count = lines.iter().len();
        let total = spacing * count.saturating_sub(1) as f64;
        let origin = e.point(10);
        // Baseline of the first line, from the attachment row.
        let first = match (attach - 1) / 3 {
            0 => origin - up * height,
            1 => origin + up * ((total + height) * 0.5 - height),
            _ => origin + up * total,
        };
        let d2 = xy(ctx.t.transform_vector3(xdir));
        let u2 = xy(ctx.t.transform_vector3(up));
        let len = d2.length();
        if len < 1e-15 {
            return;
        }
        let scale = d2.perp_dot(u2).abs() / len;
        let rotation = d2.y.atan2(d2.x);
        for (i, line) in lines.into_iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let p = first - up * (spacing * i as f64);
            let pos = xy(ctx.t.transform_point3(p));
            let text: String = line.chars().take(cc_core::cad::MAX_TEXT_CHARS).collect();
            self.emit(CadPrim::Text {
                layer,
                color,
                pos,
                height: height * scale,
                rotation,
                text,
                h,
                v: CadVAlign::Baseline,
            });
            if self.stopped {
                return;
            }
        }
    }

    fn insert(&mut self, e: &Entity, blocks: &HashMap<String, Block>, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let name = e.str(2).to_ascii_uppercase();
        let n = e.normal();
        let p = e.point(10);
        let s = DVec3::new(e.real(41, 1.0), e.real(42, 1.0), e.real(43, 1.0));
        let rot = e.real(50, 0.0).to_radians();
        let cols = e.int(70, 1).max(1) as usize;
        let rows = e.int(71, 1).max(1) as usize;
        let (cs, rs) = (e.real(44, 0.0), e.real(45, 0.0));
        // BYBLOCK entities take the insert's color (its layer's when BYLAYER).
        let block_color = color.or_else(|| self.layers.get(layer as usize).map(|l| l.color));
        let base = blocks.get(&name).map_or(DVec3::ZERO, |b| b.base);
        let placed = ctx.t * geom::ocs(n) * DAffine3::from_translation(p) * DAffine3::from_rotation_z(rot);
        'arrays: for r in 0..rows {
            for c in 0..cols {
                if self.stopped {
                    break 'arrays;
                }
                let at = placed
                    * DAffine3::from_translation(DVec3::new(c as f64 * cs, r as f64 * rs, 0.0))
                    * DAffine3::from_scale(s);
                if !(self.limits.keep_blocks && self.keep(&name, blocks, ctx, &at, layer, color)) {
                    self.block(&name, blocks, ctx, at * DAffine3::from_translation(-base), layer, block_color);
                }
                // An array of a missing block still costs work.
                self.work += 1;
                if self.work > self.limits.max_work {
                    self.stopped = true;
                }
            }
        }
        // Attributes are placed in the insert's own coordinates already.
        for attrib in &e.children {
            if attrib.name == "ATTRIB" {
                self.entity(attrib, blocks, ctx);
            }
        }
    }

    /// Keep an insert of block `name` placed by `at` (block frame, base at
    /// its origin) as a reference, when the block is named, exists, is not
    /// being inserted already and `at` is a similarity of the plan.
    fn keep(
        &mut self,
        name: &str,
        blocks: &HashMap<String, Block>,
        ctx: &Ctx,
        at: &DAffine3,
        layer: u32,
        color: Option<[u8; 3]>,
    ) -> bool {
        let anonymous = name.starts_with('*');
        if name.is_empty() || anonymous || !blocks.contains_key(name) || ctx.stack.iter().any(|s| s == name) {
            return false;
        }
        let t = ctx.t * *at;
        // The plan must stay the plan: X and Y map into it.
        if t.matrix3.x_axis.z.abs() > 1e-9 || t.matrix3.y_axis.z.abs() > 1e-9 || !t.is_finite() {
            return false;
        }
        let a = Affine2::of_plane(&t, 0.0);
        let Some(scale) = a.similarity().filter(|s| s.is_finite() && *s > 1e-12) else { return false };
        self.kept_refs += 1;
        if self.kept_refs > crate::MAX_INSERTS {
            // Kept, but not recorded: the drawing is incomplete.
            self.stopped = true;
            return true;
        }
        let x = a.m.x_axis;
        self.refs.push(CadInsert {
            block: name.to_string(),
            position: a.t,
            rotation: x.y.atan2(x.x),
            scale,
            mirrored: a.det() < 0.0,
            layer,
            color,
        });
        self.wanted.push(name.to_string());
        true
    }

    /// The entities of block `name` through `t`.
    fn block(
        &mut self,
        name: &str,
        blocks: &HashMap<String, Block>,
        ctx: &Ctx,
        t: DAffine3,
        layer: u32,
        color: Option<[u8; 3]>,
    ) {
        let Some(block) = blocks.get(name) else { return };
        if ctx.depth >= self.limits.max_depth || ctx.stack.iter().any(|s| s == name) || !t.is_finite() {
            return;
        }
        let mut stack = ctx.stack.clone();
        stack.push(name.to_string());
        let inner = Ctx { t, layer0: Some(layer), byblock: color, depth: ctx.depth + 1, stack };
        for child in &block.entities {
            if self.stopped {
                return;
            }
            self.entity(child, blocks, &inner);
        }
    }

    fn hatch(&mut self, e: &Entity, ctx: &Ctx, layer: u32, color: Option<[u8; 3]>) {
        let a = Affine2::of_plane(&(ctx.t * geom::ocs(e.normal())), e.point(10).z);
        let loops = hatch_loops(e);
        let solid = e.int(70, 0) == 1;
        let single = loops.len() == 1;
        for pts in loops {
            let pts: Vec<DVec2> = pts.into_iter().take(MAX_POLY_POINTS).map(|p| a.apply(p)).collect();
            if pts.len() < 2 {
                continue;
            }
            if solid && single && pts.len() >= 3 {
                self.emit(CadPrim::Fill { layer, color, pts: pts.clone() });
            }
            self.emit(CadPrim::Polyline { layer, color, closed: true, pts, bulges: Vec::new() });
        }
    }
}

/// Drop the references between kept blocks that close a cycle (a block
/// reaching itself through others), so the blocks nest as a tree.
fn drop_cycles(blocks: &mut [CadBlock]) {
    let index: HashMap<String, usize> = blocks.iter().enumerate().map(|(i, b)| (b.name.clone(), i)).collect();
    for i in 0..blocks.len() {
        let Some(name) = blocks.get(i).map(|b| b.name.clone()) else { continue };
        let refs: Vec<String> = blocks[i].inserts.iter().map(|r| r.block.clone()).collect();
        let keep: Vec<bool> = refs
            .iter()
            .map(|r| {
                // Does `r` reach `name`?
                let mut seen = std::collections::HashSet::new();
                let mut pending = vec![r.clone()];
                while let Some(n) = pending.pop() {
                    if n == name {
                        return false;
                    }
                    if !seen.insert(n.clone()) {
                        continue;
                    }
                    if let Some(b) = index.get(&n).and_then(|k| blocks.get(*k)) {
                        pending.extend(b.inserts.iter().map(|x| x.block.clone()));
                    }
                }
                true
            })
            .collect();
        let mut k = keep.into_iter();
        blocks[i].inserts.retain(|_| k.next().unwrap_or(false));
    }
}

fn xy(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.y)
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// A cursor over a hatch's groups, which follow a strict order.
struct Cursor<'a> {
    groups: &'a [(i32, crate::tokens::Val)],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<i32> {
        self.groups.get(self.i).map(|g| g.0)
    }

    fn peek2(&self) -> Option<i32> {
        self.groups.get(self.i + 1).map(|g| g.0)
    }

    /// The value of the next group when it has `code`.
    fn take(&mut self, code: i32) -> Option<f64> {
        let (c, v) = self.groups.get(self.i)?;
        if *c != code {
            return None;
        }
        self.i += 1;
        Some(finite(v.real()))
    }

    fn point(&mut self, code: i32) -> Option<DVec2> {
        Some(DVec2::new(self.take(code)?, self.take(code + 10)?))
    }

    fn count(&mut self, code: i32) -> Option<usize> {
        self.take(code).map(|v| (v.max(0.0) as usize).min(MAX_POLY_POINTS))
    }
}

/// The boundary loops of a hatch, tessellated, in its OCS.
fn hatch_loops(e: &Entity) -> Vec<Vec<DVec2>> {
    let mut out = Vec::new();
    let Some(start) = e.groups.iter().position(|(c, _)| *c == 91) else { return out };
    let mut cur = Cursor { groups: &e.groups, i: start };
    let Some(n) = cur.count(91) else { return out };
    for _ in 0..n.min(10_000) {
        match hatch_loop(&mut cur) {
            Some(pts) => out.push(pts),
            None => break,
        }
        // Source boundary objects.
        if let Some(k) = cur.count(97) {
            for _ in 0..k {
                if cur.peek() == Some(330) {
                    cur.i += 1;
                }
            }
        }
    }
    out
}

fn hatch_loop(cur: &mut Cursor) -> Option<Vec<DVec2>> {
    let flags = cur.take(92)? as i64;
    let mut pts: Vec<DVec2> = Vec::new();
    if flags & 2 != 0 {
        let has_bulge = cur.take(72)? != 0.0;
        let _closed = cur.take(73);
        let n = cur.count(93)?;
        let mut verts = Vec::with_capacity(n);
        for _ in 0..n {
            let p = cur.point(10)?;
            let b = if has_bulge { cur.take(42).unwrap_or(0.0) } else { 0.0 };
            verts.push((p, b));
        }
        let m = verts.len();
        for i in 0..m {
            let (p, b) = verts[i];
            let q = verts[(i + 1) % m].0;
            match geom::bulge_arc(p, q, b) {
                Some((c, r, s, sw)) => {
                    let arc = geom::arc_points(c, r, s, sw);
                    pts.extend(arc.iter().take(arc.len().saturating_sub(1)));
                }
                None => pts.push(p),
            }
            if pts.len() >= MAX_POLY_POINTS {
                break;
            }
        }
        return Some(pts);
    }
    let edges = cur.count(93)?;
    for _ in 0..edges {
        let kind = cur.take(72)? as i64;
        match kind {
            1 => {
                let a = cur.point(10)?;
                let b = cur.point(11)?;
                pts.extend([a, b]);
            }
            2 => {
                let c = cur.point(10)?;
                let r = cur.take(40)?;
                let (s, e) = (cur.take(50)?.to_radians(), cur.take(51)?.to_radians());
                let ccw = cur.take(73).unwrap_or(1.0) != 0.0;
                // Clockwise arcs store their angles mirrored.
                let (s, e) = if ccw { (s, e) } else { (-e, -s) };
                let mut sweep = (e - s).rem_euclid(TAU);
                if sweep < 1e-12 {
                    sweep = TAU;
                }
                let mut arc = geom::arc_points(c, r, s, sweep);
                if !ccw {
                    arc.reverse();
                }
                pts.extend(arc);
            }
            3 => {
                let c = cur.point(10)?;
                let major = cur.point(11)?;
                let ratio = cur.take(40)?;
                let (s, e) = (cur.take(50)?.to_radians(), cur.take(51)?.to_radians());
                let ccw = cur.take(73).unwrap_or(1.0) != 0.0;
                let (s, e) = if ccw { (s, e) } else { (-e, -s) };
                let c3 = DVec3::new(c.x, c.y, 0.0);
                let m3 = DVec3::new(major.x, major.y, 0.0);
                let mut arc: Vec<DVec2> =
                    geom::ellipse_points(c3, m3, ratio, DVec3::Z, s, e).into_iter().map(xy).collect();
                if !ccw {
                    arc.reverse();
                }
                pts.extend(arc);
            }
            4 => {
                let degree = cur.take(94)? as usize;
                let rational = cur.take(73).unwrap_or(0.0) != 0.0;
                let _periodic = cur.take(74);
                let nk = cur.count(95)?;
                let nc = cur.count(96)?;
                let mut knots = Vec::with_capacity(nk);
                for _ in 0..nk {
                    knots.push(cur.take(40)?);
                }
                let mut ctrl = Vec::with_capacity(nc);
                let mut weights = Vec::new();
                for _ in 0..nc {
                    let p = cur.point(10)?;
                    ctrl.push(DVec3::new(p.x, p.y, 0.0));
                    if rational && let Some(w) = cur.take(42) {
                        weights.push(w);
                    }
                }
                // Fit data (R2010 and later): a count, points and end tangents.
                let mut fit = Vec::new();
                if cur.peek() == Some(97) {
                    let k = cur.groups.get(cur.i).map(|g| g.1.int()).unwrap_or(0);
                    let fit_follows =
                        (k > 0 && cur.peek2() == Some(11)) || (k == 0 && matches!(cur.peek2(), Some(97 | 12)));
                    if fit_follows {
                        let k = cur.count(97)?;
                        for _ in 0..k {
                            fit.push(cur.point(11)?);
                        }
                        let _ = cur.point(12);
                        let _ = cur.point(13);
                    }
                }
                match geom::spline_points(degree, &knots, &ctrl, &weights) {
                    Some(sp) => pts.extend(sp.into_iter().map(xy)),
                    None => pts.extend(fit),
                }
            }
            _ => return None,
        }
        if pts.len() > MAX_POLY_POINTS {
            break;
        }
    }
    pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
    Some(pts)
}
