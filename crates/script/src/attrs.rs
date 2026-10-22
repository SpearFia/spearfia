//! Formatting stored in `script.md` without leaving Markdown:
//!
//! - **Styles** in the front matter, one per paragraph style, like a word processor's
//!   named styles:
//!
//!   ```yaml
//!   styles:
//!     vo: { font: "IBM Plex Sans", size: 15, color: "#E9EBEE", spacing: 1.6 }
//!     h1: { font: "Bebas Neue", size: 28, align: center }
//!   ```
//!
//! - **Paragraph overrides**: `{align=center spacing=2}` at the end of a line.
//! - **Inline spans**: `[these words]{color=#E5675F highlight=yellow size=18 font="Bebas Neue"}`.
//!
//! Keys: `font`, `size`, `color`, `highlight`, `bold`, `italic`, and for paragraphs and
//! styles also `align` (`left` | `center` | `right`) and `spacing` (line height, ×).
//! The editor hides the syntax and shows the formatting.

use std::ops::Range;

pub type Attrs = Vec<(String, String)>;

/// Keys allowed on spans and paragraphs.
pub const SPAN_KEYS: [&str; 6] = ["font", "size", "color", "highlight", "bold", "italic"];
pub const PARA_KEYS: [&str; 8] = [
    "font",
    "size",
    "color",
    "highlight",
    "bold",
    "italic",
    "align",
    "spacing",
];

/// Style keys in the front matter, matching the paragraph styles.
pub const STYLE_KEYS: [&str; 8] = ["title", "h1", "h2", "h3", "vo", "live", "onscreen", "note"];

pub fn get<'a>(attrs: &'a Attrs, key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Set or remove (`None`) a key, keeping the order of the rest.
pub fn set(attrs: &mut Attrs, key: &str, value: Option<&str>) {
    match value {
        Some(v) => match attrs.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = v.to_owned(),
            None => attrs.push((key.to_owned(), v.to_owned())),
        },
        None => attrs.retain(|(k, _)| k != key),
    }
}

fn quote(v: &str) -> String {
    if v.is_empty() || v.chars().any(|c| c.is_whitespace() || c == '}' || c == ',') {
        format!("\"{}\"", v.replace('"', ""))
    } else {
        v.to_owned()
    }
}

/// `key=value key2="two words"`, as written inside `{...}` in the text.
pub fn write_inline(attrs: &Attrs) -> String {
    attrs
        .iter()
        .map(|(k, v)| format!("{k}={}", quote(v)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `key=value key2="two words"`. Returns `None` if anything isn't `key=value` with
/// a known key, so ordinary braces in prose aren't mistaken for formatting.
pub fn parse_inline(s: &str, keys: &[&str]) -> Option<Attrs> {
    let mut out = Attrs::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        if !keys.contains(&key) {
            return None;
        }
        let after = &rest[eq + 1..];
        let (val, next) = if let Some(q) = after.strip_prefix('"') {
            let end = q.find('"')?;
            (q[..end].to_owned(), &q[end + 1..])
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (after[..end].to_owned(), &after[end..])
        };
        out.push((key.to_owned(), val));
        rest = next.trim_start();
    }
    (!out.is_empty()).then_some(out)
}

/// A `{...}` paragraph override at the end of a line: its byte range in the line
/// (including the space before it) and its attributes.
pub fn para_attrs(line: &str) -> Option<(Range<usize>, Attrs)> {
    let t = line.trim_end();
    if !t.ends_with('}') {
        return None;
    }
    let open = t.rfind('{')?;
    // Not the attributes of a cue or span (`...]{...}`), and set off by a space.
    if open > 0 && (t[..open].ends_with(']') || !t[..open].ends_with(char::is_whitespace)) {
        return None;
    }
    let attrs = parse_inline(&t[open + 1..t.len() - 1], &PARA_KEYS)?;
    let start = t[..open].trim_end().len();
    Some((start..t.len(), attrs))
}

/// An inline span `[text]{attrs}`: bracket, inner text, closing `]{...}`, attributes.
/// Ranges are absolute (offset by `base`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub open: Range<usize>,
    pub inner: Range<usize>,
    pub close: Range<usize>,
    pub attrs: Attrs,
}

/// Inline spans in a line of text. `base` is the line's byte offset.
pub fn spans(text: &str, base: usize) -> Vec<Span> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // Not a cue's `[label]` (`:name[`), not an escaped bracket.
        let cue_label = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'-');
        if b[i] == b'[' && !cue_label {
            let mut depth = 0;
            let mut j = i;
            let mut close = None;
            while j < b.len() {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if let Some(c) = close
                && b.get(c + 1) == Some(&b'{')
                && let Some(end) = text[c + 2..].find('}')
            {
                let end = c + 2 + end;
                if let Some(attrs) = parse_inline(&text[c + 2..end], &SPAN_KEYS) {
                    out.push(Span {
                        open: base + i..base + i + 1,
                        inner: base + i + 1..base + c,
                        close: base + c..base + end + 1,
                        attrs,
                    });
                    i = end + 1;
                    continue;
                }
            }
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Set (or remove) a key on the paragraph override of each line in `lines`.
pub fn set_para(src: &str, lines: Range<usize>, key: &str, value: Option<&str>) -> String {
    let mut out = String::with_capacity(src.len() + 24);
    for (i, raw) in src.split_inclusive('\n').enumerate() {
        let body = raw.trim_end_matches(['\n', '\r']);
        if lines.contains(&i) && !body.trim().is_empty() && !body.trim_start().starts_with("---") {
            let (text, mut attrs) = match para_attrs(body) {
                Some((r, a)) => (&body[..r.start], a),
                None => (body.trim_end(), Attrs::new()),
            };
            set(&mut attrs, key, value);
            out.push_str(text);
            if !attrs.is_empty() {
                out.push_str(" {");
                out.push_str(&write_inline(&attrs));
                out.push('}');
            }
        } else {
            out.push_str(body);
        }
        out.push_str(&raw[body.len()..]);
    }
    out
}

/// Remove the paragraph override from line `n`, returning it.
pub fn take_para(src: &str, n: usize) -> (String, Attrs) {
    let mut taken = Attrs::new();
    let mut out = String::with_capacity(src.len());
    for (i, raw) in src.split_inclusive('\n').enumerate() {
        let body = raw.trim_end_matches(['\n', '\r']);
        match (i == n).then(|| para_attrs(body)).flatten() {
            Some((r, a)) => {
                taken = a;
                out.push_str(&body[..r.start]);
            }
            None => out.push_str(body),
        }
        out.push_str(&raw[body.len()..]);
    }
    (out, taken)
}

/// Set (or remove) a key on the text in `sel`. If `sel` is (inside) an existing span, that
/// span changes; otherwise the selection is wrapped in a new one. A span left with no
/// attributes is unwrapped. Returns the new source and the range of the same text.
pub fn set_span(
    src: &str,
    sel: Range<usize>,
    key: &str,
    value: Option<&str>,
) -> (String, Range<usize>) {
    let ls = src[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    let le = src[sel.end..].find('\n').map_or(src.len(), |i| sel.end + i);
    let existing = spans(&src[ls..le], ls)
        .into_iter()
        .find(|s| s.inner.start <= sel.start && sel.end <= s.inner.end);
    if let Some(s) = existing {
        let mut attrs = s.attrs.clone();
        set(&mut attrs, key, value);
        let inner = &src[s.inner.clone()];
        if attrs.is_empty() {
            let out = format!("{}{inner}{}", &src[..s.open.start], &src[s.close.end..]);
            return (out, s.open.start..s.open.start + inner.len());
        }
        let out = format!(
            "{}[{inner}]{{{}}}{}",
            &src[..s.open.start],
            write_inline(&attrs),
            &src[s.close.end..]
        );
        return (out, s.inner.clone());
    }
    let Some(v) = value else {
        return (src.to_owned(), sel);
    };
    let inner = &src[sel.clone()];
    let out = format!(
        "{}[{inner}]{{{key}={}}}{}",
        &src[..sel.start],
        quote(v),
        &src[sel.end..]
    );
    (out, sel.start + 1..sel.end + 1)
}

/// The `styles:` block of the front matter: style key → attributes.
pub fn styles(front_matter_raw: &str) -> Vec<(String, Attrs)> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in front_matter_raw.lines() {
        if l.trim_end() == "styles:" {
            inside = true;
            continue;
        }
        if inside {
            if !l.starts_with(' ') && !l.starts_with('\t') {
                break;
            }
            let t = l.trim();
            let Some((key, map)) = t.split_once(':') else {
                continue;
            };
            let map = map.trim();
            let Some(body) = map.strip_prefix('{').and_then(|m| m.strip_suffix('}')) else {
                continue;
            };
            out.push((key.trim().to_owned(), parse_flow_map(body)));
        }
    }
    out
}

/// `font: "IBM Plex Sans", size: 15` → attributes.
fn parse_flow_map(body: &str) -> Attrs {
    let mut out = Attrs::new();
    let mut rest = body.trim();
    while !rest.is_empty() {
        let Some(colon) = rest.find(':') else { break };
        let key = rest[..colon].trim().to_owned();
        let after = rest[colon + 1..].trim_start();
        let (val, next) = if let Some(q) = after.strip_prefix('"') {
            let end = q.find('"').unwrap_or(q.len());
            (q[..end].to_owned(), q.get(end + 1..).unwrap_or(""))
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (after[..end].trim().to_owned(), &after[end..])
        };
        out.push((key, val));
        rest = next.trim_start().trim_start_matches(',').trim_start();
    }
    out
}

fn write_flow_map(attrs: &Attrs) -> String {
    let parts: Vec<String> = attrs
        .iter()
        .map(|(k, v)| {
            let num = v.parse::<f32>().is_ok() || v == "true" || v == "false";
            let plain = num || matches!(v.as_str(), "left" | "center" | "right");
            if plain {
                format!("{k}: {v}")
            } else {
                format!("{k}: \"{v}\"")
            }
        })
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

/// Replace one style in the front matter (`None` removes it), creating the front matter
/// and the `styles:` block if needed.
pub fn set_style(src: &str, key: &str, attrs: Option<&Attrs>) -> String {
    let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let line = attrs
        .filter(|a| !a.is_empty())
        .map(|a| format!("  {key}: {}", write_flow_map(a)));
    if !src.starts_with("---") {
        return match line {
            Some(l) => format!("---{nl}styles:{nl}{l}{nl}---{nl}{src}"),
            None => src.to_owned(),
        };
    }
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim_end() == "---")
        .map(|(i, _)| i);
    let Some(close) = close else {
        return src.to_owned();
    };
    let header = lines
        .iter()
        .take(close)
        .position(|l| l.trim_end() == "styles:");
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    match header {
        Some(h) => {
            // The block runs while lines are indented.
            let mut end = h + 1;
            while end < close && (out[end].starts_with(' ') || out[end].starts_with('\t')) {
                end += 1;
            }
            let existing = (h + 1..end).find(|&i| {
                out[i]
                    .trim()
                    .split_once(':')
                    .is_some_and(|(k, _)| k.trim() == key)
            });
            match (existing, line) {
                (Some(i), Some(l)) => out[i] = format!("{l}{nl}"),
                (Some(i), None) => {
                    out.remove(i);
                    if end - h == 2 {
                        out.remove(h);
                    }
                }
                (None, Some(l)) => out.insert(end, format!("{l}{nl}")),
                (None, None) => {}
            }
        }
        None => {
            if let Some(l) = line {
                out.insert(close, format!("{l}{nl}"));
                out.insert(close, format!("styles:{nl}"));
            }
        }
    }
    out.concat()
}

/// A color value (`#RRGGBB`, `#RGB` or a name) as RGB.
pub fn color(v: &str) -> Option<[u8; 3]> {
    let v = v.trim();
    if let Some(h) = v.strip_prefix('#') {
        let p = |s: &str| u8::from_str_radix(s, 16).ok();
        return match h.len() {
            6 => Some([p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?]),
            3 => {
                let d = |i: usize| p(&h[i..i + 1]).map(|x| x * 17);
                Some([d(0)?, d(1)?, d(2)?])
            }
            _ => None,
        };
    }
    Some(match v.to_ascii_lowercase().as_str() {
        "red" => [0xE5, 0x67, 0x5F],
        "orange" => [0xF0, 0x8C, 0x3A],
        "yellow" => [0xF2, 0xD0, 0x4F],
        "green" => [0x4C, 0xC3, 0x8A],
        "teal" => [0x5C, 0xC8, 0xE0],
        "blue" => [0x6F, 0x9B, 0xFF],
        "purple" => [0xA8, 0x8B, 0xFA],
        "pink" => [0xE5, 0x8B, 0xC0],
        "gray" | "grey" => [0xA2, 0xA8, 0xB0],
        "white" => [0xFF, 0xFF, 0xFF],
        "black" => [0x10, 0x12, 0x14],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spans_but_not_cues_or_links() {
        let t = "say [this part]{color=red size=18} and :sfx[boom]{#s1} [docs](x) [a]{nope=1}";
        let s = spans(t, 0);
        assert_eq!(s.len(), 1);
        assert_eq!(&t[s[0].inner.clone()], "this part");
        assert_eq!(&t[s[0].close.clone()], "]{color=red size=18}");
        assert_eq!(get(&s[0].attrs, "size"), Some("18"));
        let q = spans("[x]{font=\"Bebas Neue\"}", 10);
        assert_eq!(get(&q[0].attrs, "font"), Some("Bebas Neue"));
        assert_eq!(q[0].inner, 11..12);
    }

    #[test]
    fn paragraph_attrs() {
        let (r, a) = para_attrs("Hello there {align=center spacing=2}").unwrap();
        assert_eq!(r, 11..36);
        assert_eq!(get(&a, "align"), Some("center"));
        assert!(para_attrs(":sfx[boom]{#s1}").is_none());
        assert!(para_attrs("[x]{color=red}").is_none());
        assert!(para_attrs("set {x} in prose").is_none());
        assert!(para_attrs("code {foo=1}").is_none(), "unknown key");

        let src = "a\nb {align=center}\n";
        let s = set_para(src, 0..2, "spacing", Some("1.5"));
        assert_eq!(s, "a {spacing=1.5}\nb {align=center spacing=1.5}\n");
        let s = set_para(&s, 0..2, "spacing", None);
        assert_eq!(s, "a\nb {align=center}\n");
        let (s, taken) = take_para(&s, 1);
        assert_eq!(s, "a\nb\n");
        assert_eq!(get(&taken, "align"), Some("center"));
    }

    #[test]
    fn sets_spans() {
        let src = "say hello there";
        let (s, r) = set_span(src, 4..9, "color", Some("red"));
        assert_eq!(s, "say [hello]{color=red} there");
        assert_eq!(&s[r.clone()], "hello");
        let (s, r) = set_span(&s, r, "size", Some("18"));
        assert_eq!(s, "say [hello]{color=red size=18} there");
        let (s, r) = set_span(&s, r, "color", None);
        let (s, r) = set_span(&s, r, "size", None);
        assert_eq!(s, src);
        assert_eq!(&s[r], "hello");
        let (s, _) = set_span(src, 4..9, "font", Some("Bebas Neue"));
        assert_eq!(s, "say [hello]{font=\"Bebas Neue\"} there");
    }

    #[test]
    fn styles_in_front_matter() {
        let fm = "title: x\nstyles:\n  vo: { font: \"IBM Plex Sans\", size: 15, color: \"#E9EBEE\" }\n  h1: { size: 28, align: center }\nformat: long\n";
        let s = styles(fm);
        assert_eq!(s.len(), 2);
        assert_eq!(get(&s[0].1, "font"), Some("IBM Plex Sans"));
        assert_eq!(get(&s[1].1, "align"), Some("center"));

        let src = "---\ntitle: x\n---\nbody\n";
        let a: Attrs = vec![
            ("size".into(), "28".into()),
            ("font".into(), "Bebas Neue".into()),
        ];
        let out = set_style(src, "h1", Some(&a));
        assert_eq!(
            out,
            "---\ntitle: x\nstyles:\n  h1: { size: 28, font: \"Bebas Neue\" }\n---\nbody\n"
        );
        let b: Attrs = vec![("color".into(), "#E9EBEE".into())];
        let out = set_style(&out, "vo", Some(&b));
        assert!(out.contains(
            "  h1: { size: 28, font: \"Bebas Neue\" }\n  vo: { color: \"#E9EBEE\" }\n---"
        ));
        let out = set_style(&out, "h1", None);
        let out = set_style(&out, "vo", None);
        assert_eq!(out, src);
        assert_eq!(
            set_style("body\n", "vo", Some(&b)),
            "---\nstyles:\n  vo: { color: \"#E9EBEE\" }\n---\nbody\n"
        );
    }

    #[test]
    fn colors() {
        assert_eq!(color("#E5675F"), Some([0xE5, 0x67, 0x5F]));
        assert_eq!(color("#fff"), Some([255, 255, 255]));
        assert_eq!(color("Yellow"), Some([0xF2, 0xD0, 0x4F]));
        assert_eq!(color("nope"), None);
    }
}
