//! Text strings of drawings: control codes and MTEXT formatting.

/// Replace `\U+XXXX` escapes and the `%%` control codes of single-line
/// text (`%%c` diameter, `%%d` degree, `%%p` plus-minus, `%%nnn` a
/// character code; `%%u`/`%%o` underline and overline toggles dropped).
pub fn plain(s: &str) -> String {
    let unicode = unescape_unicode(s);
    let chars: Vec<char> = unicode.chars().collect();
    let mut out = String::with_capacity(unicode.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && chars.get(i + 1) == Some(&'%') {
            match chars.get(i + 2).map(|c| c.to_ascii_lowercase()) {
                Some('c') => out.push('\u{2300}'),
                Some('d') => out.push('°'),
                Some('p') => out.push('±'),
                Some('%') => out.push('%'),
                Some('u' | 'o' | 'k') => {}
                Some(d) if d.is_ascii_digit() => {
                    let digits: String = chars[i + 2..].iter().take(3).take_while(|c| c.is_ascii_digit()).collect();
                    if let Some(c) = digits.parse::<u32>().ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                    i += 2 + digits.len();
                    continue;
                }
                _ => {
                    out.push('%');
                    i += 1;
                    continue;
                }
            }
            i += 3;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `\U+XXXX` (a Unicode character) escapes.
fn unescape_unicode(s: &str) -> String {
    if !s.contains("\\U+") && !s.contains("\\u+") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && matches!(chars.get(i + 1), Some('U' | 'u')) && chars.get(i + 2) == Some(&'+') {
            let hex: String = chars.get(i + 3..i + 7).map(|h| h.iter().collect()).unwrap_or_default();
            if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(c);
                i += 7;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The lines of an MTEXT string without its formatting: `\P` breaks lines,
/// `\~` is a space, `\\ \{ \}` are literal, stacked fractions `\Sa^b;`
/// become `a/b`, other codes (`\A1;`, `\H2x;`, `\fArial|b0;`, `\C1;`, `\L`…)
/// and grouping braces are removed.
pub fn mtext_lines(s: &str) -> Vec<String> {
    let s = unescape_unicode(s);
    let chars: Vec<char> = s.chars().collect();
    let mut lines = vec![String::new()];
    let mut i = 0;
    let push = |lines: &mut Vec<String>, c: char| {
        if let Some(l) = lines.last_mut() {
            l.push(c);
        }
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' | '}' => i += 1,
            '\\' => {
                let Some(&code) = chars.get(i + 1) else { break };
                i += 2;
                match code {
                    'P' | 'X' => lines.push(String::new()),
                    '~' => push(&mut lines, ' '),
                    '\\' | '{' | '}' => push(&mut lines, code),
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' => {}
                    'S' => {
                        // Stacked text up to ';': "a^b", "a/b" or "a#b".
                        let end = chars[i..].iter().position(|&c| c == ';').map_or(chars.len(), |p| i + p);
                        let stacked: String = chars[i..end].iter().collect();
                        for ch in stacked.replace(['^', '#'], "/").chars() {
                            push(&mut lines, ch);
                        }
                        i = (end + 1).min(chars.len());
                    }
                    // Codes with a value up to ';'.
                    'A' | 'C' | 'c' | 'F' | 'f' | 'H' | 'h' | 'Q' | 'q' | 'T' | 't' | 'W' | 'w' | 'p' => {
                        let end = chars[i..].iter().position(|&c| c == ';').map_or(chars.len(), |p| i + p);
                        i = (end + 1).min(chars.len());
                    }
                    other => push(&mut lines, other),
                }
            }
            '\n' => {
                lines.push(String::new());
                i += 1;
            }
            '\r' => i += 1,
            _ => {
                push(&mut lines, c);
                i += 1;
            }
        }
    }
    lines.into_iter().map(|l| plain(&l)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_codes() {
        assert_eq!(plain("%%c100"), "\u{2300}100");
        assert_eq!(plain("45%%d %%p0.5 100%%%"), "45° ±0.5 100%");
        assert_eq!(plain("\\U+00C7A\\U+00C3O"), "ÇAÃO");
        assert_eq!(plain("50%"), "50%");
    }

    #[test]
    fn mtext_formatting_is_stripped() {
        assert_eq!(mtext_lines("{\\fArial|b1;Sala}\\P\\H2.5x;12,5 m\\U+00B2"), ["Sala", "12,5 m²"]);
        assert_eq!(mtext_lines("\\A1;1\\S1^2; pol"), ["11/2 pol"]);
        assert_eq!(mtext_lines("a\\~b \\{c\\} \\\\"), ["a b {c} \\"]);
        assert_eq!(mtext_lines("\\Lunder\\l\\C1;red"), ["underred"]);
        // Unterminated codes end quietly.
        assert_eq!(mtext_lines("x\\H2.5"), ["x"]);
        assert_eq!(mtext_lines("y\\"), ["y"]);
    }
}
