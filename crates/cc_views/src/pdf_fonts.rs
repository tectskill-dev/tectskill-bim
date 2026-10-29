//! Fonts embedded in exported PDFs for the text Helvetica cannot show (Thai,
//! Lao, CJK, …): the text is shaped ([`crate::shaping`]) and its glyphs are
//! drawn from subsets of Inter, Sarabun and the system's fallback fonts,
//! written as CID fonts (Identity-H) with a ToUnicode map so the text can
//! still be searched and copied.

use crate::shaping;
use skrifa::{FontRef, MetadataProvider};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Thai letters (SIL Open Font License, see `assets/fonts/OFL-Sarabun.txt`).
pub(crate) const SARABUN: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-Regular.ttf");

/// The fonts drawn text is set in, in fallback order: Inter, Sarabun, then
/// the system's fallbacks (CJK, Lao), as font data and face index.
pub(crate) fn sources() -> Vec<(&'static [u8], u32)> {
    let mut out: Vec<(&'static [u8], u32)> = vec![(crate::export::INTER, 0), (SARABUN, 0)];
    out.extend(crate::fonts::fallbacks().map(|f| (f.data.as_slice(), f.index)));
    out
}

/// Readable faces of `sources`, with the index of each in `sources`.
pub(crate) fn faces(sources: &[(&'static [u8], u32)]) -> (Vec<FontRef<'static>>, Vec<usize>) {
    let mut fonts = Vec::new();
    let mut origin = Vec::new();
    for (i, (data, index)) in sources.iter().enumerate() {
        if let Ok(f) = FontRef::from_index(data, *index) {
            fonts.push(f);
            origin.push(i);
        }
    }
    (fonts, origin)
}

/// A glyph to draw: the font's resource index (`/T<k>`), its CID and its
/// place in ems from the start of the baseline.
pub(crate) struct Placed {
    pub resource: usize,
    pub cid: u16,
    pub x: f64,
    pub y: f64,
}

struct Used {
    face: usize,
    remap: subsetter::GlyphRemapper,
    /// Advance of each CID, in 1/1000 em.
    widths: BTreeMap<u16, f64>,
    unicode: BTreeMap<u16, String>,
}

pub(crate) struct EmbeddedFonts {
    sources: Vec<(&'static [u8], u32)>,
    fonts: Vec<FontRef<'static>>,
    origin: Vec<usize>,
    used: Vec<Used>,
}

impl EmbeddedFonts {
    pub fn new() -> Self {
        let sources = sources();
        let (fonts, origin) = faces(&sources);
        EmbeddedFonts { sources, fonts, origin, used: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }

    /// `text` shaped, as glyphs to draw and its width in ems.
    pub fn shape(&mut self, text: &str) -> (Vec<Placed>, f64) {
        use skrifa::instance::{LocationRef, Size};
        let (glyphs, width) = shaping::shape(&self.fonts, text);
        let mut out = Vec::with_capacity(glyphs.len());
        for g in glyphs {
            let resource = match self.used.iter().position(|u| u.face == g.font) {
                Some(i) => i,
                None => {
                    self.used.push(Used {
                        face: g.font,
                        remap: subsetter::GlyphRemapper::new(),
                        widths: BTreeMap::new(),
                        unicode: BTreeMap::new(),
                    });
                    self.used.len() - 1
                }
            };
            let font = &self.fonts[g.font];
            let used = &mut self.used[resource];
            let old = u16::try_from(g.glyph.to_u32()).unwrap_or(0);
            let cid = used.remap.remap(old);
            used.widths.entry(cid).or_insert_with(|| {
                let upem = font.metrics(Size::unscaled(), LocationRef::default()).units_per_em.max(1) as f64;
                let advance =
                    font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(g.glyph).unwrap_or(0.0)
                        as f64;
                advance * 1000.0 / upem
            });
            // A glyph seen for a single character keeps that one over the
            // clusters it was first seen in (the text itself also goes to
            // the page as ActualText).
            if let Some(t) = g.text.filter(|t| !t.is_empty()) {
                let single = t.chars().count() == 1;
                match used.unicode.get(&cid) {
                    Some(old) if !single || old.chars().count() == 1 => {}
                    _ => {
                        used.unicode.insert(cid, t);
                    }
                }
            }
            out.push(Placed { resource, cid, x: g.x, y: g.y });
        }
        (out, width)
    }

    /// The page's font resources, for fonts written from object `first`.
    pub fn resources(&self, first: usize) -> String {
        let mut s = String::new();
        for k in 0..self.used.len() {
            let _ = write!(s, "/T{k} {} 0 R ", first + 5 * k);
        }
        s
    }

    /// The PDF objects of the used fonts, numbered from `first`: five per
    /// font (Type0 font, CID font, descriptor, font file, ToUnicode map).
    /// A font that cannot be subset is written without its file, so viewers
    /// substitute one rather than the export failing.
    pub fn objects(&self, first: usize) -> Vec<Vec<u8>> {
        use skrifa::instance::{LocationRef, Size};
        let mut out = Vec::new();
        for (k, used) in self.used.iter().enumerate() {
            let base = first + 5 * k;
            let font = &self.fonts[used.face];
            let (data, index) = self.sources[self.origin[used.face]];
            let cff = font.table_data(skrifa::Tag::new(b"CFF ")).is_some();
            let name = format!("{}+{}", subset_tag(k), postscript_name(font, k));
            let m = font.metrics(Size::unscaled(), LocationRef::default());
            let em = 1000.0 / m.units_per_em.max(1) as f64;
            let bbox = m.bounds.map_or([0.0, m.descent as f64, 1000.0, m.ascent as f64], |b| {
                [b.x_min as f64, b.y_min as f64, b.x_max as f64, b.y_max as f64]
            });
            let file = subsetter::subset(data, index, &used.remap).ok();

            out.push(
                format!(
                    "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>",
                    base + 1,
                    base + 4
                )
                .into_bytes(),
            );
            let mut widths = String::new();
            for (cid, w) in &used.widths {
                let _ = write!(widths, "{cid} [{}] ", w.round() as i64);
            }
            out.push(
                format!(
                    "<< /Type /Font /Subtype /{} /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 0 /W [{widths}]{} >>",
                    if cff { "CIDFontType0" } else { "CIDFontType2" },
                    base + 2,
                    if cff { "" } else { " /CIDToGIDMap /Identity" }
                )
                .into_bytes(),
            );
            let file_key = if cff { "FontFile3" } else { "FontFile2" };
            let file_ref = if file.is_some() { format!(" /{file_key} {} 0 R", base + 3) } else { String::new() };
            out.push(
                format!(
                    "<< /Type /FontDescriptor /FontName /{name} /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80{file_ref} >>",
                    (bbox[0] * em).round(),
                    (bbox[1] * em).round(),
                    (bbox[2] * em).round(),
                    (bbox[3] * em).round(),
                    (m.ascent as f64 * em).round(),
                    (m.descent as f64 * em).round(),
                    (m.cap_height.unwrap_or(m.ascent * 0.7) as f64 * em).round()
                )
                .into_bytes(),
            );
            let file = file.unwrap_or_default();
            let dict = if cff {
                format!("<< /Length {} /Subtype /OpenType >>", file.len())
            } else {
                format!("<< /Length {} /Length1 {} >>", file.len(), file.len())
            };
            out.push(stream(dict, &file));
            let cmap = to_unicode(&used.unicode);
            out.push(stream(format!("<< /Length {} >>", cmap.len()), cmap.as_bytes()));
        }
        out
    }
}

fn stream(dict: String, data: &[u8]) -> Vec<u8> {
    let mut s = dict.into_bytes();
    s.extend(b"\nstream\n");
    s.extend(data);
    s.extend(b"\nendstream");
    s
}

/// Six capital letters naming the `k`th subset (PDF's `ABCDEF+Name`).
fn subset_tag(k: usize) -> String {
    let mut n = k;
    let mut tag = [b'A'; 6];
    for slot in tag.iter_mut().rev() {
        *slot = b'A' + (n % 26) as u8;
        n /= 26;
    }
    tag[0] = b'C';
    String::from_utf8_lossy(&tag).into_owned()
}

/// The font's PostScript name, reduced to characters PDF names allow.
fn postscript_name(font: &FontRef, k: usize) -> String {
    let name = font
        .localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME)
        .english_or_first()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    if clean.is_empty() { format!("Font{k}") } else { clean }
}

/// The ToUnicode CMap from CIDs to the text they stand for.
fn to_unicode(map: &BTreeMap<u16, String>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = map.iter().collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (cid, text) in chunk {
            let hex: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{cid:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
    s
}
