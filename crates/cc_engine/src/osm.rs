//! OpenStreetMap data around the project site: the JSON an Overpass query
//! returns, turned into a site drawing (buildings, streets, paths,
//! railways, water, green areas, trees and street names, in project
//! coordinates) and building footprints with heights for context masses;
//! plus the answers of the address search (Nominatim) and of an elevation
//! service. Everything here is pure: the program downloads, the engine
//! reads. Map data is © OpenStreetMap contributors, under the ODbL.

use cc_core::cad::{CadDrawing, CadHAlign, CadLayer, CadPrim, CadUnit, CadVAlign};
use cc_core::site::{SiteAddress, SiteLocation};
use glam::DVec2;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Largest Overpass answer read, bytes.
pub const MAX_JSON: usize = 64 * 1024 * 1024;
/// Most OSM elements read from one answer.
pub const MAX_ELEMENTS: usize = 500_000;
/// Most points in one way or ring.
const MAX_WAY_POINTS: usize = 20_000;
/// Most member ways joined into the rings of one relation.
const MAX_MEMBERS: usize = 2_000;
/// Fewest meters a street runs before it gets a name label.
const MIN_LABEL_RUN: f64 = 25.0;
/// Radius of the circle drawn for a tree, meters.
const TREE_RADIUS: f64 = 2.0;
/// Most points kept in a mass outline (see [`cc_core::mass::MAX_OUTLINE`]).
const MAX_OUTLINE: usize = cc_core::mass::MAX_OUTLINE;
/// Attribution required with any use of OpenStreetMap data.
pub const ATTRIBUTION: &str = "© OpenStreetMap contributors";

/// The layers of a site drawing, in the order they are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    Buildings,
    Streets,
    Paths,
    Railways,
    Water,
    Green,
    Trees,
    Names,
}

impl Layer {
    pub const ALL: [Layer; 8] = [
        Layer::Buildings,
        Layer::Streets,
        Layer::Paths,
        Layer::Railways,
        Layer::Water,
        Layer::Green,
        Layer::Trees,
        Layer::Names,
    ];

    /// The localization key of its name.
    pub fn key(self) -> &'static str {
        match self {
            Layer::Buildings => "osm-layer-buildings",
            Layer::Streets => "osm-layer-streets",
            Layer::Paths => "osm-layer-paths",
            Layer::Railways => "osm-layer-railways",
            Layer::Water => "osm-layer-water",
            Layer::Green => "osm-layer-green",
            Layer::Trees => "osm-layer-trees",
            Layer::Names => "osm-layer-names",
        }
    }

    /// Short code in command results.
    pub fn code(self) -> &'static str {
        match self {
            Layer::Buildings => "buildings",
            Layer::Streets => "streets",
            Layer::Paths => "paths",
            Layer::Railways => "railways",
            Layer::Water => "water",
            Layer::Green => "green",
            Layer::Trees => "trees",
            Layer::Names => "names",
        }
    }

    fn color(self) -> [u8; 3] {
        match self {
            Layer::Buildings => [90, 90, 90],
            Layer::Streets => [200, 120, 40],
            Layer::Paths => [170, 110, 150],
            Layer::Railways => [80, 80, 140],
            Layer::Water => [60, 130, 200],
            Layer::Green => [80, 160, 80],
            Layer::Trees => [60, 140, 60],
            Layer::Names => [40, 40, 40],
        }
    }
}

// --- Overpass JSON ---

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    elements: Vec<RawElement>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct LatLon {
    lat: f64,
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct RawMember {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(rename = "ref", default)]
    id: i64,
    #[serde(default)]
    role: String,
    #[serde(default)]
    geometry: Option<Vec<Option<LatLon>>>,
}

#[derive(Debug, Deserialize)]
struct RawElement {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: i64,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    nodes: Vec<i64>,
    #[serde(default)]
    geometry: Option<Vec<Option<LatLon>>>,
    #[serde(default)]
    members: Vec<RawMember>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

/// What a way or area is, from its tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Building,
    Street,
    Path,
    Railway,
    Water,
    Green,
}

fn classify(tags: &HashMap<String, String>) -> Option<Class> {
    let tag = |k: &str| tags.get(k).map(String::as_str);
    if tag("building").is_some_and(|v| v != "no") {
        return Some(Class::Building);
    }
    if let Some(h) = tag("highway") {
        return match h {
            "footway" | "path" | "cycleway" | "steps" | "pedestrian" | "track" | "bridleway" | "corridor" => {
                Some(Class::Path)
            }
            "motorway" | "trunk" | "primary" | "secondary" | "tertiary" | "unclassified" | "residential"
            | "service" | "living_street" | "road" | "busway" | "motorway_link" | "trunk_link" | "primary_link"
            | "secondary_link" | "tertiary_link" => Some(Class::Street),
            _ => None,
        };
    }
    if tag("railway").is_some_and(|v| matches!(v, "rail" | "tram" | "light_rail" | "subway" | "narrow_gauge")) {
        return Some(Class::Railway);
    }
    if tag("waterway").is_some() || tag("natural") == Some("water") || tag("water").is_some() {
        return Some(Class::Water);
    }
    let green = matches!(tag("leisure"), Some("park" | "garden" | "pitch" | "playground"))
        || matches!(tag("landuse"), Some("grass" | "forest" | "meadow" | "recreation_ground" | "village_green"))
        || matches!(tag("natural"), Some("wood" | "scrub" | "grassland"));
    green.then_some(Class::Green)
}

/// A building footprint for a context mass.
#[derive(Debug, Clone, PartialEq)]
pub struct Building {
    /// Footprint in project coordinates, open (first point not repeated).
    pub outline: Vec<DVec2>,
    /// Height above its base, meters.
    pub height: f64,
    /// Height of its base above the ground, meters (`min_height`).
    pub base: f64,
    /// Its height came from the map (`height` or `building:levels`), not the default.
    pub measured: bool,
    /// Name, else the street address, if the map gives one.
    pub label: String,
    /// `way/123` or `relation/45`.
    pub source: String,
}

/// How map data becomes a site drawing and masses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Half the side of the square kept around the site, meters (`None`: everything given).
    pub extent: Option<f64>,
    /// Height of a storey, for `building:levels`.
    pub level_height: f64,
    /// Storeys of a building whose height the map does not give.
    pub default_levels: f64,
    /// Height of street names, meters.
    pub text_height: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { extent: None, level_height: 3.0, default_levels: 2.0, text_height: 2.0 }
    }
}

/// A site drawing and the buildings for masses.
#[derive(Debug, Clone, Default)]
pub struct Site {
    pub drawing: CadDrawing,
    pub buildings: Vec<Building>,
    /// Primitives per layer.
    pub counts: Vec<(Layer, usize)>,
}

/// The ways and areas of an answer, in project coordinates.
struct Reader<'a> {
    site: &'a SiteLocation,
    nodes: HashMap<i64, DVec2>,
    ways: HashMap<i64, Vec<DVec2>>,
}

impl Reader<'_> {
    fn points(&self, geometry: &Option<Vec<Option<LatLon>>>, nodes: &[i64]) -> Vec<DVec2> {
        let mut pts: Vec<DVec2> = match geometry {
            Some(g) => g
                .iter()
                .take(MAX_WAY_POINTS)
                .flatten()
                .filter(|p| on_earth(p.lat, p.lon))
                .map(|p| self.site.to_project(p.lat, p.lon))
                .collect(),
            None => nodes.iter().take(MAX_WAY_POINTS).filter_map(|n| self.nodes.get(n).copied()).collect(),
        };
        pts.dedup_by(|a, b| a.distance_squared(*b) < 1e-8);
        pts
    }
}

/// Read an Overpass answer (`[out:json]`, with `out geom` or with the
/// ways' nodes) into a site drawing in project coordinates around `site`.
pub fn read(json: &str, site: &SiteLocation, opts: &Options) -> Result<Site, String> {
    if json.len() > MAX_JSON {
        return Err(format!("the map data is larger than {} MB", MAX_JSON / (1024 * 1024)));
    }
    let answer: Answer = serde_json::from_str(json).map_err(|e| format!("not Overpass JSON: {e}"))?;
    if answer.elements.len() > MAX_ELEMENTS {
        return Err(format!("the map data has more than {MAX_ELEMENTS} elements"));
    }
    let mut r = Reader { site, nodes: HashMap::new(), ways: HashMap::new() };
    for e in answer.elements.iter().filter(|e| e.kind == "node") {
        if let (Some(lat), Some(lon)) = (e.lat, e.lon)
            && on_earth(lat, lon)
        {
            r.nodes.insert(e.id, site.to_project(lat, lon));
        }
    }
    for e in answer.elements.iter().filter(|e| e.kind == "way") {
        let pts = r.points(&e.geometry, &e.nodes);
        r.ways.insert(e.id, pts);
    }

    let mut b = Builder::new(opts);
    let mut labels: Vec<(String, Vec<DVec2>)> = Vec::new();
    for e in &answer.elements {
        match e.kind.as_str() {
            "node" => {
                if e.tags.get("natural").map(String::as_str) == Some("tree")
                    && let Some(p) = r.nodes.get(&e.id)
                    && b.inside(*p)
                {
                    b.tree(*p);
                }
            }
            "way" => {
                let Some(class) = classify(&e.tags) else { continue };
                let Some(pts) = r.ways.get(&e.id) else { continue };
                let closed = e.nodes.len() > 3 && e.nodes.first() == e.nodes.last()
                    || e.geometry.is_some() && pts.len() > 2 && is_ring(pts, &e.geometry);
                match class {
                    Class::Building if closed => {
                        b.building(open_ring(pts), &e.tags, format!("way/{}", e.id));
                    }
                    Class::Street | Class::Path | Class::Railway => {
                        b.line(class, pts);
                        if class == Class::Street
                            && let Some(name) = e.tags.get("name").filter(|n| !n.trim().is_empty())
                        {
                            labels.push((name.trim().to_string(), pts.clone()));
                        }
                    }
                    Class::Water | Class::Green if closed => b.area(class, open_ring(pts)),
                    Class::Water => b.line(class, pts),
                    _ => {}
                }
            }
            "relation" => {
                if !matches!(e.tags.get("type").map(String::as_str), Some("multipolygon" | "building")) {
                    continue;
                }
                let Some(class @ (Class::Building | Class::Water | Class::Green)) = classify(&e.tags) else { continue };
                let parts: Vec<Vec<DVec2>> = e
                    .members
                    .iter()
                    .filter(|m| m.kind == "way" && (m.role == "outer" || m.role.is_empty()))
                    .take(MAX_MEMBERS)
                    .map(|m| match &m.geometry {
                        Some(_) => r.points(&m.geometry, &[]),
                        None => r.ways.get(&m.id).cloned().unwrap_or_default(),
                    })
                    .filter(|p| p.len() >= 2)
                    .collect();
                for ring in join_rings(parts) {
                    match class {
                        Class::Building => b.building(ring, &e.tags, format!("relation/{}", e.id)),
                        _ => b.area(class, ring),
                    }
                }
            }
            _ => {}
        }
    }
    b.labels(labels);
    Ok(b.finish())
}

/// Finite coordinates within latitude ±90° and longitude ±180°.
fn on_earth(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
}

/// `pts` (from `out geom`) closes on itself.
fn is_ring(pts: &[DVec2], geometry: &Option<Vec<Option<LatLon>>>) -> bool {
    let Some(g) = geometry else { return false };
    let (Some(Some(a)), Some(Some(z))) = (g.first(), g.last()) else { return false };
    g.len() > 3 && a.lat == z.lat && a.lon == z.lon || pts.len() > 2 && pts[0].distance(pts[pts.len() - 1]) < 1e-6
}

/// A closed ring without its repeated last point.
fn open_ring(pts: &[DVec2]) -> Vec<DVec2> {
    let mut ring = pts.to_vec();
    if ring.len() > 1 && ring[0].distance(ring[ring.len() - 1]) < 1e-4 {
        ring.pop();
    }
    ring
}

/// Join open member ways end to end into closed rings (rings already
/// closed are kept); ways that close nothing are dropped.
fn join_rings(parts: Vec<Vec<DVec2>>) -> Vec<Vec<DVec2>> {
    const TOL: f64 = 1e-4;
    let mut rings = Vec::new();
    let mut open: Vec<Vec<DVec2>> = Vec::new();
    for p in parts {
        if p.len() > 3 && p[0].distance(p[p.len() - 1]) < TOL {
            rings.push(open_ring(&p));
        } else {
            open.push(p);
        }
    }
    while let Some(mut ring) = open.pop() {
        let mut guard = 0;
        loop {
            guard += 1;
            let (Some(first), Some(last)) = (ring.first().copied(), ring.last().copied()) else { break };
            if ring.len() > 3 && first.distance(last) < TOL {
                rings.push(open_ring(&ring));
                break;
            }
            let next = open.iter().position(|p| {
                p.first().is_some_and(|a| a.distance(last) < TOL) || p.last().is_some_and(|z| z.distance(last) < TOL)
            });
            let Some(i) = next.filter(|_| guard <= MAX_MEMBERS && ring.len() < MAX_WAY_POINTS) else { break };
            let mut p = open.swap_remove(i);
            if p.first().is_some_and(|a| a.distance(last) >= TOL) {
                p.reverse();
            }
            ring.extend(p.into_iter().skip(1));
        }
    }
    rings
}

/// A number at the start of a tag value, in meters (`12`, `12.5 m`, `40 ft`, `40'`).
fn meters(value: &str) -> Option<f64> {
    let v = value.trim().replace(',', ".");
    let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(v.len());
    let n: f64 = v.get(..end)?.parse().ok()?;
    let rest = v.get(end..).unwrap_or_default().trim();
    let n = if rest.starts_with("ft") || rest.starts_with('\'') { n * 0.3048 } else { n };
    (n.is_finite() && n >= 0.0).then_some(n)
}

struct Builder<'o> {
    opts: &'o Options,
    prims: Vec<CadPrim>,
    buildings: Vec<Building>,
    seen_buildings: HashSet<String>,
}

impl<'o> Builder<'o> {
    fn new(opts: &'o Options) -> Self {
        Builder { opts, prims: Vec::new(), buildings: Vec::new(), seen_buildings: HashSet::new() }
    }

    fn inside(&self, p: DVec2) -> bool {
        self.opts.extent.is_none_or(|h| p.x.abs() <= h && p.y.abs() <= h)
    }

    fn layer_of(class: Class) -> Layer {
        match class {
            Class::Building => Layer::Buildings,
            Class::Street => Layer::Streets,
            Class::Path => Layer::Paths,
            Class::Railway => Layer::Railways,
            Class::Water => Layer::Water,
            Class::Green => Layer::Green,
        }
    }

    fn polyline(&mut self, layer: Layer, pts: Vec<DVec2>, closed: bool) {
        if pts.len() >= 2 {
            self.prims.push(CadPrim::Polyline { layer: layer as u32, color: None, closed, pts, bulges: Vec::new() });
        }
    }

    /// An open line, cut to the kept square.
    fn line(&mut self, class: Class, pts: &[DVec2]) {
        let layer = Self::layer_of(class);
        for piece in clip_polyline(pts, self.opts.extent) {
            self.polyline(layer, piece, false);
        }
    }

    /// A closed area: kept whole when any of it is in the square, else as the pieces of its edge inside.
    fn area(&mut self, class: Class, ring: Vec<DVec2>) {
        if ring.len() < 3 {
            return;
        }
        let layer = Self::layer_of(class);
        match self.opts.extent {
            Some(h) if !ring.iter().all(|p| p.x.abs() <= h && p.y.abs() <= h) => {
                let mut closed = ring.clone();
                closed.push(ring[0]);
                for piece in clip_polyline(&closed, Some(h)) {
                    self.polyline(layer, piece, false);
                }
            }
            _ => self.polyline(layer, ring, true),
        }
    }

    fn tree(&mut self, p: DVec2) {
        self.prims.push(CadPrim::Arc {
            layer: Layer::Trees as u32,
            color: None,
            center: p,
            radius: TREE_RADIUS,
            start: 0.0,
            sweep: std::f64::consts::TAU,
        });
    }

    fn building(&mut self, ring: Vec<DVec2>, tags: &HashMap<String, String>, source: String) {
        if ring.len() < 3 || ring.len() > MAX_OUTLINE || cc_core::mass::area(&ring) < 1.0 {
            return;
        }
        let centroid = ring.iter().copied().sum::<DVec2>() / ring.len() as f64;
        if !self.inside(centroid) || !self.seen_buildings.insert(source.clone()) {
            return;
        }
        let tag = |k: &str| tags.get(k).map(String::as_str);
        let base = tag("min_height").and_then(meters).unwrap_or(0.0);
        let levels = tag("building:levels").and_then(meters).filter(|l| *l >= 1.0);
        let (top, measured) = match (tag("height").and_then(meters), levels) {
            (Some(h), _) if h > base => (h, true),
            (_, Some(l)) => (base + l * self.opts.level_height, true),
            _ => (base + self.opts.default_levels * self.opts.level_height, false),
        };
        let label = match (tag("name"), tag("addr:street"), tag("addr:housenumber")) {
            (Some(n), _, _) if !n.trim().is_empty() => n.trim().to_string(),
            (_, Some(s), Some(h)) => format!("{s}, {h}"),
            (_, _, Some(h)) => h.to_string(),
            _ => String::new(),
        };
        self.polyline(Layer::Buildings, ring.clone(), true);
        self.buildings.push(Building { outline: ring, height: (top - base).max(0.5), base, measured, label, source });
    }

    /// One name per street run long enough, along its longest stretch inside the square, reading left to right.
    fn labels(&mut self, streets: Vec<(String, Vec<DVec2>)>) {
        let h = self.opts.text_height;
        let mut placed: Vec<(String, DVec2)> = Vec::new();
        for (name, pts) in streets {
            for piece in clip_polyline(&pts, self.opts.extent) {
                let run: f64 = piece.windows(2).map(|w| w[0].distance(w[1])).sum();
                if run < MIN_LABEL_RUN.max(cc_core::cad::text_width(&name, h) * 1.2) {
                    continue;
                }
                let Some((a, b)) = piece
                    .windows(2)
                    .map(|w| (w[0], w[1]))
                    .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1)))
                else {
                    continue;
                };
                let mid = (a + b) * 0.5;
                // The same name close by already has a label.
                if placed.iter().any(|(n, p)| *n == name && p.distance(mid) < 4.0 * MIN_LABEL_RUN) {
                    continue;
                }
                let mut angle = (b - a).to_angle();
                if angle > std::f64::consts::FRAC_PI_2 {
                    angle -= std::f64::consts::PI;
                } else if angle < -std::f64::consts::FRAC_PI_2 {
                    angle += std::f64::consts::PI;
                }
                self.prims.push(CadPrim::Text {
                    layer: Layer::Names as u32,
                    color: None,
                    pos: mid,
                    height: h,
                    rotation: angle,
                    text: name.clone(),
                    h: CadHAlign::Center,
                    v: CadVAlign::Middle,
                });
                placed.push((name.clone(), mid));
            }
        }
    }

    fn finish(self) -> Site {
        let layers = Layer::ALL
            .iter()
            .map(|l| CadLayer { name: crate::i18n::tr(l.key()), color: l.color(), visible: true })
            .collect();
        let counts =
            Layer::ALL.iter().map(|l| (*l, self.prims.iter().filter(|p| p.layer() == *l as u32).count())).collect();
        let mut drawing =
            CadDrawing { layers, prims: self.prims, images: Vec::new(), extents: None, units: Some(CadUnit::Meters) };
        drawing.compute_extents();
        Site { drawing, buildings: self.buildings, counts }
    }
}

/// The pieces of a polyline inside the square `|x|, |y| <= extent` (all of it with no extent).
fn clip_polyline(pts: &[DVec2], extent: Option<f64>) -> Vec<Vec<DVec2>> {
    let Some(h) = extent else {
        return if pts.len() >= 2 { vec![pts.to_vec()] } else { Vec::new() };
    };
    let mut out: Vec<Vec<DVec2>> = Vec::new();
    let mut current: Vec<DVec2> = Vec::new();
    for w in pts.windows(2) {
        match clip_segment(w[0], w[1], h) {
            Some((a, b)) => {
                if current.last().is_none_or(|l| l.distance(a) > 1e-9) {
                    if current.len() >= 2 {
                        out.push(std::mem::take(&mut current));
                    }
                    current = vec![a];
                }
                current.push(b);
            }
            None => {
                if current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        out.push(current);
    }
    out
}

/// Liang–Barsky: the part of segment `a`–`b` inside the square of half side `h`.
fn clip_segment(a: DVec2, b: DVec2, h: f64) -> Option<(DVec2, DVec2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d.x, a.x + h), (d.x, h - a.x), (-d.y, a.y + h), (d.y, h - a.y)] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    Some((a + d * t0, a + d * t1))
}

/// The Overpass QL query for the map data of the square of half side
/// `extent` meters around `site` (its bounding box in latitude and
/// longitude, widened to hold the square under any north angle).
pub fn overpass_query(site: &SiteLocation, extent: f64) -> String {
    let r = extent * std::f64::consts::SQRT_2;
    let (s, w) = site.lat_lon(DVec2::new(-r, -r));
    let (n, e) = site.lat_lon(DVec2::new(r, r));
    let bbox = format!("{s:.7},{w:.7},{n:.7},{e:.7}");
    format!(
        "[out:json][timeout:60];(\
         way[\"building\"]({bbox});relation[\"building\"][\"type\"~\"multipolygon|building\"]({bbox});\
         way[\"highway\"]({bbox});way[\"railway\"]({bbox});\
         way[\"waterway\"]({bbox});way[\"natural\"~\"water|wood|scrub|grassland\"]({bbox});\
         relation[\"natural\"=\"water\"]({bbox});\
         way[\"leisure\"~\"park|garden|pitch|playground\"]({bbox});\
         way[\"landuse\"~\"grass|forest|meadow|recreation_ground|village_green\"]({bbox});\
         node[\"natural\"=\"tree\"]({bbox}););out geom;"
    )
}

// --- Address search and elevation ---

/// A place the address search found.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The full name the service gives.
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub address: SiteAddress,
}

#[derive(Debug, Deserialize)]
struct RawPlace {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    lat: String,
    #[serde(default)]
    lon: String,
    #[serde(default)]
    address: HashMap<String, String>,
}

impl RawPlace {
    fn place(self) -> Option<Place> {
        let latitude: f64 = self.lat.trim().parse().ok()?;
        let longitude: f64 = self.lon.trim().parse().ok()?;
        if !SiteLocation::at(latitude, longitude).is_valid() {
            return None;
        }
        let a = &self.address;
        let first = |keys: &[&str]| keys.iter().find_map(|k| a.get(*k)).cloned().unwrap_or_default();
        Some(Place {
            name: self.display_name,
            latitude,
            longitude,
            address: SiteAddress {
                street: first(&["road", "pedestrian", "footway", "street"]),
                number: first(&["house_number"]),
                district: first(&["suburb", "neighbourhood", "quarter", "city_district", "borough"]),
                city: first(&["city", "town", "village", "municipality", "hamlet"]),
                region: first(&["state", "region", "province", "county"]),
                postcode: first(&["postcode"]),
                country: first(&["country"]),
            },
        })
    }
}

/// The places of a Nominatim search answer (`format=jsonv2`, `addressdetails=1`).
pub fn read_places(json: &str) -> Result<Vec<Place>, String> {
    let raw: Vec<RawPlace> = serde_json::from_str(json).map_err(|e| format!("not a search answer: {e}"))?;
    Ok(raw.into_iter().take(50).filter_map(RawPlace::place).collect())
}

/// The place of a Nominatim reverse answer (`format=jsonv2`), if any.
pub fn read_reverse(json: &str) -> Result<Option<Place>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not a search answer: {e}"))?;
    if value.get("error").is_some() {
        return Ok(None);
    }
    let raw: RawPlace = serde_json::from_value(value).map_err(|e| format!("not a search answer: {e}"))?;
    Ok(raw.place())
}

/// The first elevation of an elevation answer (`{"elevation": [12.0]}`), meters.
pub fn read_elevation(json: &str) -> Result<f64, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not an elevation answer: {e}"))?;
    let z = value
        .get("elevation")
        .and_then(|v| v.as_array().and_then(|a| a.first()).or(Some(v)))
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| "the answer gives no elevation".to_string())?;
    if z.is_finite() && cc_core::site::ELEVATION_RANGE.contains(&z) {
        Ok(z)
    } else {
        Err(format!("elevation out of range: {z}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> SiteLocation {
        SiteLocation::at(-3.7319, -38.5267)
    }

    /// A point `dx`, `dy` meters east and north of the site.
    fn ll(dx: f64, dy: f64) -> String {
        let (lat, lon) = site().lat_lon(DVec2::new(dx, dy));
        format!("{{\"lat\":{lat},\"lon\":{lon}}}")
    }

    fn square(x: f64, y: f64, s: f64) -> String {
        [ll(x, y), ll(x + s, y), ll(x + s, y + s), ll(x, y + s), ll(x, y)].join(",")
    }

    #[test]
    fn buildings_streets_and_names() {
        let json = format!(
            r#"{{"elements":[
            {{"type":"way","id":1,"tags":{{"building":"yes","building:levels":"4","name":"Edifício A"}},"geometry":[{}]}},
            {{"type":"way","id":2,"tags":{{"building":"house"}},"geometry":[{}]}},
            {{"type":"way","id":3,"tags":{{"highway":"residential","name":"Rua das Flores"}},"geometry":[{},{}]}},
            {{"type":"way","id":4,"tags":{{"highway":"footway"}},"geometry":[{},{}]}},
            {{"type":"node","id":5,"lat":{},"lon":{},"tags":{{"natural":"tree"}}}},
            {{"type":"way","id":6,"tags":{{"building":"yes","height":"18 m","min_height":"3"}},"geometry":[{}]}}
            ]}}"#,
            square(10.0, 10.0, 20.0),
            square(-40.0, -40.0, 10.0),
            ll(-100.0, 0.0),
            ll(100.0, 0.0),
            ll(0.0, -50.0),
            ll(0.0, 50.0),
            site().lat_lon(DVec2::new(5.0, 5.0)).0,
            site().lat_lon(DVec2::new(5.0, 5.0)).1,
            square(50.0, -20.0, 8.0),
        );
        let s = read(&json, &site(), &Options::default()).unwrap();
        assert_eq!(s.buildings.len(), 3);
        let a = &s.buildings[0];
        assert!((a.height - 12.0).abs() < 1e-9 && a.measured);
        assert_eq!(a.label, "Edifício A");
        assert_eq!(a.outline.len(), 4);
        assert!((cc_core::mass::area(&a.outline).abs() - 400.0).abs() < 0.5, "{}", cc_core::mass::area(&a.outline));
        assert!(!s.buildings[1].measured);
        assert!((s.buildings[1].height - 6.0).abs() < 1e-9);
        assert!((s.buildings[2].base - 3.0).abs() < 1e-9 && (s.buildings[2].height - 15.0).abs() < 1e-9);
        let count = |l: Layer| s.counts.iter().find(|(x, _)| *x == l).map_or(0, |c| c.1);
        assert_eq!(count(Layer::Buildings), 3);
        assert_eq!(count(Layer::Streets), 1);
        assert_eq!(count(Layer::Paths), 1);
        assert_eq!(count(Layer::Trees), 1);
        assert_eq!(count(Layer::Names), 1);
        let text = s.drawing.prims.iter().find_map(|p| match p {
            CadPrim::Text { text, pos, rotation, .. } => Some((text.clone(), *pos, *rotation)),
            _ => None,
        });
        let (text, pos, rotation) = text.unwrap();
        assert_eq!(text, "Rua das Flores");
        assert!(pos.length() < 0.5 && rotation.abs() < 1e-6, "{pos} {rotation}");
        assert_eq!(s.drawing.units, Some(CadUnit::Meters));
        assert_eq!(s.drawing.layers.len(), Layer::ALL.len());
    }

    #[test]
    fn extent_clips_lines_and_drops_far_buildings() {
        let json = format!(
            r#"{{"elements":[
            {{"type":"way","id":1,"tags":{{"building":"yes"}},"geometry":[{}]}},
            {{"type":"way","id":2,"tags":{{"highway":"primary"}},"geometry":[{},{}]}}
            ]}}"#,
            square(300.0, 300.0, 10.0),
            ll(-500.0, 10.0),
            ll(500.0, 10.0),
        );
        let opts = Options { extent: Some(100.0), ..Options::default() };
        let s = read(&json, &site(), &opts).unwrap();
        assert!(s.buildings.is_empty());
        let CadPrim::Polyline { pts, .. } = &s.drawing.prims[0] else { panic!("expected a polyline") };
        assert!((pts[0].x + 100.0).abs() < 1e-6 && (pts[1].x - 100.0).abs() < 1e-6, "{pts:?}");
    }

    #[test]
    fn multipolygon_outer_ways_join_into_a_ring() {
        let json = format!(
            r#"{{"elements":[
            {{"type":"relation","id":9,"tags":{{"type":"multipolygon","building":"yes"}},"members":[
              {{"type":"way","ref":1,"role":"outer","geometry":[{},{},{}]}},
              {{"type":"way","ref":2,"role":"outer","geometry":[{},{},{}]}},
              {{"type":"way","ref":3,"role":"inner","geometry":[{}]}}
            ]}}]}}"#,
            ll(0.0, 0.0),
            ll(10.0, 0.0),
            ll(10.0, 10.0),
            ll(0.0, 0.0),
            ll(0.0, 10.0),
            ll(10.0, 10.0),
            square(2.0, 2.0, 2.0),
        );
        let s = read(&json, &site(), &Options::default()).unwrap();
        assert_eq!(s.buildings.len(), 1);
        assert_eq!(s.buildings[0].outline.len(), 4);
        assert_eq!(s.buildings[0].source, "relation/9");
    }

    #[test]
    fn ways_by_node_reference() {
        let n = |id: i64, dx: f64, dy: f64| {
            let (lat, lon) = site().lat_lon(DVec2::new(dx, dy));
            format!(r#"{{"type":"node","id":{id},"lat":{lat},"lon":{lon}}}"#)
        };
        let json = format!(
            r#"{{"elements":[{},{},{},{},{{"type":"way","id":7,"nodes":[1,2,3,4,1],"tags":{{"building":"yes"}}}}]}}"#,
            n(1, 0.0, 0.0),
            n(2, 6.0, 0.0),
            n(3, 6.0, 6.0),
            n(4, 0.0, 6.0)
        );
        let s = read(&json, &site(), &Options::default()).unwrap();
        assert_eq!(s.buildings.len(), 1);
    }

    #[test]
    fn hostile_input_is_refused_or_skipped() {
        assert!(read("not json", &site(), &Options::default()).is_err());
        let json = r#"{"elements":[{"type":"way","id":1,"tags":{"building":"yes"},"geometry":[null,{"lat":1000,"lon":0},{"lat":0,"lon":-500}]},
            {"type":"node","id":2,"lat":null,"tags":{"natural":"tree"}},
            {"type":"relation","id":3,"tags":{"type":"multipolygon","building":"yes"},"members":[{"type":"way","ref":99}]}]}"#;
        let s = read(json, &site(), &Options::default()).unwrap();
        assert!(s.buildings.is_empty() && s.drawing.prims.is_empty());
        // Numbers past f64 make the answer unreadable as a whole.
        assert!(
            read(r#"{"elements":[{"type":"node","id":1,"lat":1e400,"lon":0}]}"#, &site(), &Options::default()).is_err()
        );
    }

    #[test]
    fn tag_lengths() {
        assert_eq!(meters("12"), Some(12.0));
        assert_eq!(meters("12,5 m"), Some(12.5));
        assert!((meters("10 ft").unwrap_or(0.0) - 3.048).abs() < 1e-9);
        assert_eq!(meters("tall"), None);
    }

    #[test]
    fn search_reverse_and_elevation_answers() {
        let json = r#"[{"display_name":"Rua A, 10, Centro, Fortaleza","lat":"-3.73","lon":"-38.52",
            "address":{"road":"Rua A","house_number":"10","suburb":"Centro","city":"Fortaleza","state":"Ceará","postcode":"60000-000","country":"Brasil"}},
            {"display_name":"bad","lat":"x","lon":"0"}]"#;
        let places = read_places(json).unwrap();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].address.city, "Fortaleza");
        assert_eq!(places[0].address.region, "Ceará");
        assert!(read_reverse(r#"{"error":"Unable to geocode"}"#).unwrap().is_none());
        assert_eq!(read_elevation(r#"{"elevation":[21.0]}"#), Ok(21.0));
        assert!(read_elevation(r#"{"elevation":[99999]}"#).is_err());
    }

    #[test]
    fn query_names_the_box() {
        let q = overpass_query(&site(), 100.0);
        assert!(q.starts_with("[out:json]") && q.ends_with("out geom;"));
        assert!(q.contains("-3.73"));
    }
}
