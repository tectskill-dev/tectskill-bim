//! DXF group-code pairs, from ASCII or binary files.
//!
//! A DXF file is a sequence of (group code, value) pairs; the code decides
//! the value's type. ASCII files put each on its own line; binary files
//! (after the sentinel `AutoCAD Binary DXF\r\n\x1a\0`) store the code as a
//! little-endian 16-bit integer and the value in its binary form.

use crate::CadError;

/// The sentinel that starts a binary DXF file.
pub const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF\r\n\x1a\0";

/// A group value.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    Real(f64),
    Int(i64),
    /// Binary chunks (thumbnails, proxy data): ignored.
    Bin,
}

impl Val {
    pub fn real(&self) -> f64 {
        match self {
            Val::Real(v) => *v,
            Val::Int(i) => *i as f64,
            Val::Str(s) => s.trim().parse().unwrap_or(0.0),
            Val::Bin => 0.0,
        }
    }

    pub fn int(&self) -> i64 {
        match self {
            Val::Int(i) => *i,
            Val::Real(v) if v.is_finite() => *v as i64,
            Val::Str(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn str(&self) -> &str {
        match self {
            Val::Str(s) => s,
            _ => "",
        }
    }
}

/// How a group code's value is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Str,
    Real,
    I16,
    I32,
    I64,
    Bool,
    Bin,
}

fn kind(code: i32) -> Kind {
    match code {
        0..=9 | 100..=109 | 300..=309 | 320..=369 | 390..=399 | 410..=419 | 430..=439 | 470..=481 | 999 => Kind::Str,
        1000..=1003 | 1005..=1009 => Kind::Str,
        310..=319 | 1004 => Kind::Bin,
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Kind::Real,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => Kind::I16,
        90..=99 | 420..=429 | 440..=459 | 1071 => Kind::I32,
        160..=169 => Kind::I64,
        290..=299 => Kind::Bool,
        _ => Kind::Str,
    }
}

/// Decodes strings: UTF-8 when valid, else the drawing's code page.
pub struct Decoder {
    encoding: &'static encoding_rs::Encoding,
}

impl Default for Decoder {
    fn default() -> Self {
        Self { encoding: encoding_rs::WINDOWS_1252 }
    }
}

impl Decoder {
    /// Use the code page named by `$DWGCODEPAGE` (e.g. `ANSI_1252`).
    pub fn set_code_page(&mut self, name: &str) {
        let n = name.trim().to_ascii_uppercase();
        let number = n.trim_start_matches("ANSI_").trim_start_matches("DOS");
        let label = match number {
            "932" => "shift_jis".to_string(),
            "936" => "gbk".to_string(),
            "949" => "euc-kr".to_string(),
            "950" => "big5".to_string(),
            "866" => "ibm866".to_string(),
            "874" => "windows-874".to_string(),
            n if n.starts_with("125") => format!("windows-{n}"),
            _ => return,
        };
        if let Some(e) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            self.encoding = e;
        }
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => self.encoding.decode_without_bom_handling(bytes).0.into_owned(),
        }
    }
}

/// Reads pairs one at a time, with one pair of look-ahead.
pub struct Pairs<'a> {
    data: &'a [u8],
    pos: usize,
    binary: bool,
    peeked: Option<(i32, Val)>,
    pub decoder: Decoder,
}

impl<'a> Pairs<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let binary = data.starts_with(BINARY_SENTINEL);
        let mut pos = if binary { BINARY_SENTINEL.len() } else { 0 };
        // A UTF-8 byte order mark before an ASCII file.
        if !binary && data.starts_with(&[0xEF, 0xBB, 0xBF]) {
            pos = 3;
        }
        Self { data, pos, binary, peeked: None, decoder: Decoder::default() }
    }

    pub fn is_binary(&self) -> bool {
        self.binary
    }

    /// The next pair, or `None` at the end of the data.
    pub fn next_pair(&mut self) -> Result<Option<(i32, Val)>, CadError> {
        if let Some(p) = self.peeked.take() {
            return Ok(Some(p));
        }
        if self.binary { self.next_binary() } else { self.next_ascii() }
    }

    /// Look at the next pair without consuming it.
    pub fn peek(&mut self) -> Result<Option<&(i32, Val)>, CadError> {
        if self.peeked.is_none() {
            self.peeked = if self.binary { self.next_binary()? } else { self.next_ascii()? };
        }
        Ok(self.peeked.as_ref())
    }

    fn line(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.data.len() {
            return None;
        }
        let rest = self.data.get(self.pos..)?;
        let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        self.pos += end + 1;
        let mut line = rest.get(..end)?;
        while let [head @ .., b'\r'] = line {
            line = head;
        }
        Some(line)
    }

    fn next_ascii(&mut self) -> Result<Option<(i32, Val)>, CadError> {
        // Skip blank lines between pairs (some writers leave a trailing one).
        let code_line = loop {
            match self.line() {
                None => return Ok(None),
                Some(l) if l.iter().all(u8::is_ascii_whitespace) => continue,
                Some(l) => break l,
            }
        };
        let code_text = std::str::from_utf8(code_line).map_err(|_| self.error("a group code is not a number"))?;
        let code: i32 = code_text.trim().parse().map_err(|_| self.error("a group code is not a number"))?;
        let Some(value) = self.line() else {
            return Err(self.error("the file ends after a group code"));
        };
        let val = match kind(code) {
            Kind::Str => Val::Str(self.decoder.decode(value)),
            Kind::Bin => Val::Bin,
            Kind::Real => Val::Real(parse_ascii(value).ok_or_else(|| self.error("a number cannot be read"))?),
            Kind::I16 | Kind::I32 | Kind::I64 | Kind::Bool => {
                let v = parse_ascii(value).ok_or_else(|| self.error("an integer cannot be read"))?;
                Val::Int(if v.is_finite() { v.clamp(i64::MIN as f64, i64::MAX as f64) as i64 } else { 0 })
            }
        };
        Ok(Some((code, val)))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CadError> {
        let end = self.pos.checked_add(n).ok_or_else(|| self.error("the file is truncated"))?;
        let bytes = self.data.get(self.pos..end).ok_or_else(|| self.error("the file is truncated"))?;
        self.pos = end;
        Ok(bytes)
    }

    fn next_binary(&mut self) -> Result<Option<(i32, Val)>, CadError> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let c = self.take(2)?;
        let code = i32::from(i16::from_le_bytes([c[0], c[1]]));
        let val = match kind(code) {
            Kind::Str => {
                let rest = self.data.get(self.pos..).unwrap_or_default();
                let end = rest.iter().position(|&b| b == 0).ok_or_else(|| self.error("the file is truncated"))?;
                let s = self.decoder.decode(rest.get(..end).unwrap_or_default());
                self.pos += end + 1;
                Val::Str(s)
            }
            Kind::Bin => {
                let n = self.take(1)?[0] as usize;
                self.take(n)?;
                Val::Bin
            }
            Kind::Real => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Val::Real(f64::from_le_bytes(a))
            }
            Kind::I16 => {
                let b = self.take(2)?;
                Val::Int(i64::from(i16::from_le_bytes([b[0], b[1]])))
            }
            Kind::I32 => {
                let b = self.take(4)?;
                Val::Int(i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])))
            }
            Kind::I64 => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Val::Int(i64::from_le_bytes(a))
            }
            Kind::Bool => Val::Int(i64::from(self.take(1)?[0])),
        };
        Ok(Some((code, val)))
    }

    fn error(&self, what: &str) -> CadError {
        CadError::Format(format!("{what} (byte {})", self.pos))
    }
}

fn parse_ascii(bytes: &[u8]) -> Option<f64> {
    let s = std::str::from_utf8(bytes).ok()?.trim();
    if s.is_empty() {
        return Some(0.0);
    }
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ascii_pairs() {
        let mut p = Pairs::new(b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\n 10\r\n1.5\r\n 70\r\n   3\r\n");
        assert_eq!(p.next_pair().unwrap(), Some((0, Val::Str("SECTION".into()))));
        assert_eq!(p.peek().unwrap().map(|x| x.0), Some(2));
        assert_eq!(p.next_pair().unwrap(), Some((2, Val::Str("ENTITIES".into()))));
        assert_eq!(p.next_pair().unwrap(), Some((10, Val::Real(1.5))));
        assert_eq!(p.next_pair().unwrap(), Some((70, Val::Int(3))));
        assert_eq!(p.next_pair().unwrap(), None);
    }

    #[test]
    fn reads_binary_pairs() {
        let mut data = BINARY_SENTINEL.to_vec();
        data.extend(0i16.to_le_bytes());
        data.extend(b"LINE\0");
        data.extend(10i16.to_le_bytes());
        data.extend(2.5f64.to_le_bytes());
        data.extend(62i16.to_le_bytes());
        data.extend(7i16.to_le_bytes());
        data.extend(290i16.to_le_bytes());
        data.push(1);
        let mut p = Pairs::new(&data);
        assert!(p.is_binary());
        assert_eq!(p.next_pair().unwrap(), Some((0, Val::Str("LINE".into()))));
        assert_eq!(p.next_pair().unwrap(), Some((10, Val::Real(2.5))));
        assert_eq!(p.next_pair().unwrap(), Some((62, Val::Int(7))));
        assert_eq!(p.next_pair().unwrap(), Some((290, Val::Int(1))));
        assert_eq!(p.next_pair().unwrap(), None);
        // Truncated values are errors, not panics.
        let mut p = Pairs::new(&data[..data.len() - 3]);
        while let Ok(Some(_)) = p.next_pair() {}
    }

    #[test]
    fn decodes_code_page_text() {
        let mut d = Decoder::default();
        assert_eq!(d.decode(b"Cora\xe7\xe3o"), "Coração");
        d.set_code_page("ANSI_1251");
        assert_eq!(d.decode(b"\xc4\xe0"), "Да");
        assert_eq!(d.decode("já".as_bytes()), "já", "valid UTF-8 stays UTF-8");
    }
}
