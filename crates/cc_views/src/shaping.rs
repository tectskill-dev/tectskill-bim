//! Text shaping for the exports that draw glyphs themselves (PDF with
//! embedded fonts, PNG outlines): marks, ligatures and kerning come from the
//! fonts' OpenType tables through HarfRust, so scripts that stack vowels and
//! tone marks (Thai, Lao) are placed as the font designs them.

use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// One positioned glyph, in ems from the start of the text's baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaped {
    /// Index of the font in the list given to [`shape`].
    pub font: usize,
    pub glyph: GlyphId,
    pub x: f64,
    pub y: f64,
    /// The characters this glyph stands for (for text extraction): its own
    /// character when a cluster has a glyph per character, else the whole
    /// cluster on its first glyph and `None` on the others.
    pub text: Option<String>,
}

/// `text` shaped with `fonts`: each run of characters is set in one font,
/// the first that has its first character, and the run goes on while that
/// font has the next ones (so marks, spaces and digits stay with the letters
/// before them). Characters no font has show the first font's missing-glyph
/// box. Returns the glyphs and the total advance, in ems.
pub fn shape(fonts: &[FontRef], text: &str) -> (Vec<Shaped>, f64) {
    let mut out = Vec::new();
    let mut pen = 0.0;
    if fonts.is_empty() {
        return (out, pen);
    }
    let charmaps: Vec<_> = fonts.iter().map(|f| f.charmap()).collect();
    let has = |f: usize, c: char| charmaps[f].map(c).is_some();
    // Runs as (font, start byte, end byte).
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        if let Some(last) = runs.last_mut()
            && has(last.0, c)
        {
            last.2 = end;
            continue;
        }
        let font = (0..fonts.len()).find(|f| has(*f, c)).unwrap_or(0);
        match runs.last_mut() {
            Some(last) if last.0 == font => last.2 = end,
            _ => runs.push((font, i, end)),
        }
    }
    for (font, start, end) in runs {
        let run = &text[start..end];
        let face = &fonts[font];
        let upem = face.head().map(|h| h.units_per_em()).unwrap_or(1000).max(1) as f64;
        let data = harfrust::ShaperData::new(face);
        let shaper = data.shaper(face).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(run);
        buffer.guess_segment_properties();
        let glyphs = shaper.shape(buffer, harfrust::ShapeOptions::new());
        let infos = glyphs.glyph_infos();
        let positions = glyphs.glyph_positions();
        // Cluster starts, to cut the text each cluster stands for.
        let mut starts: Vec<usize> = infos.iter().map(|g| g.cluster as usize).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut i = 0;
        while i < infos.len() {
            let cluster = infos[i].cluster as usize;
            let count = infos[i..].iter().take_while(|g| g.cluster as usize == cluster).count();
            let next = starts.iter().copied().find(|s| *s > cluster).unwrap_or(run.len());
            let chars: Vec<char> = run.get(cluster..next).unwrap_or("").chars().collect();
            for k in 0..count {
                // As many glyphs as characters: one each (Thai letters and
                // marks); otherwise the first glyph stands for them all.
                let text = if chars.len() == count {
                    Some(chars[k].to_string())
                } else if k == 0 {
                    Some(chars.iter().collect())
                } else {
                    None
                };
                let (info, pos) = (&infos[i + k], &positions[i + k]);
                out.push(Shaped {
                    font,
                    glyph: GlyphId::new(info.glyph_id),
                    x: pen + pos.x_offset as f64 / upem,
                    y: pos.y_offset as f64 / upem,
                    text,
                });
                pen += pos.x_advance as f64 / upem;
            }
            i += count;
        }
    }
    (out, pen)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
    const SARABUN: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-Regular.ttf");

    #[test]
    fn runs_switch_fonts_and_keep_marks_with_their_letters() {
        let fonts = [FontRef::new(INTER).unwrap(), FontRef::new(SARABUN).unwrap()];
        let (glyphs, width) = shape(&fonts, "Wall ผนังที่ 2");
        assert!(width > 0.0);
        // Latin from Inter, Thai (with the space and digit after it) from Sarabun.
        assert_eq!(glyphs[0].font, 0);
        assert!(glyphs.iter().skip(5).all(|g| g.font == 1));
        assert!(glyphs.iter().all(|g| g.glyph != GlyphId::NOTDEF));
        // Over an upper vowel ("ที่") the font sets the tone mark higher than
        // over a bare letter ("ท่"), by substitution or by offset.
        let (high, _) = shape(&fonts[1..], "ที่");
        let (low, _) = shape(&fonts[1..], "ท่");
        assert_eq!((high.len(), low.len()), (3, 2));
        assert_ne!((high[2].glyph, high[2].y), (low[1].glyph, low[1].y), "{high:?} {low:?}");
        // Every character is kept for text extraction.
        let joined: String = glyphs.iter().filter_map(|g| g.text.clone()).collect();
        assert_eq!(joined, "Wall ผนังที่ 2");
    }

    #[test]
    fn missing_characters_use_the_first_font() {
        let fonts = [FontRef::new(INTER).unwrap()];
        let (glyphs, _) = shape(&fonts, "ก");
        assert_eq!(glyphs.len(), 1);
        assert_eq!((glyphs[0].font, glyphs[0].glyph), (0, GlyphId::NOTDEF));
    }
}
