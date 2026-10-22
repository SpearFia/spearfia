//! Small text edits to `script.md` that keep the rest of the file byte for byte: add a
//! cue to a line, change a cue's label, remove a cue. Each returns the new source; the
//! caller re-parses and saves.

use crate::{Block, Directive, LineKind, Script, Segment};

impl Directive {
    /// The directive as written in `script.md`: `:name[label]{key=value #id}`.
    pub fn to_source(&self) -> String {
        let mut s = format!(":{}", self.name);
        if let Some(l) = &self.label {
            s.push('[');
            s.push_str(l);
            s.push(']');
        }
        let mut inner: Vec<String> = self
            .attrs
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    k.clone()
                } else if v.chars().any(char::is_whitespace) {
                    format!("{k}=\"{v}\"")
                } else {
                    format!("{k}={v}")
                }
            })
            .collect();
        if let Some(id) = &self.id {
            inner.push(format!("#{id}"));
        }
        if !inner.is_empty() || self.label.is_none() {
            s.push('{');
            s.push_str(&inner.join(" "));
            s.push('}');
        }
        s
    }
}

/// Every directive in the script, in order.
pub fn directives(script: &Script) -> impl Iterator<Item = &Directive> {
    script.blocks.iter().flat_map(|b| -> Vec<&Directive> {
        match b {
            Block::Heading { cues, .. } => cues.iter().collect(),
            Block::Divider { cue, .. } => vec![cue],
            Block::Line { segments, .. } => segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Cue(d) => Some(d),
                    Segment::Text(_) => None,
                })
                .collect(),
        }
    })
}

/// The next free id with this prefix: `m1`, `m2`, ... One past the highest number in
/// use; if that would overflow, the lowest unused number. Never reuses an id.
pub fn next_id(script: &Script, prefix: &str) -> String {
    let used: std::collections::HashSet<&str> =
        directives(script).filter_map(|d| d.id.as_deref()).collect();
    let max = used
        .iter()
        .filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    if let Some(n) = max.checked_add(1) {
        let id = format!("{prefix}{n}");
        if !used.contains(id.as_str()) {
            return id;
        }
    }
    // A script has far fewer cues than numbers, so this finds a gap quickly.
    (1..=u64::MAX)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !used.contains(id.as_str()))
        .expect("fewer ids in use than u64::MAX")
}

/// Make a label safe to put between `[` and `]`.
pub fn clean_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | '{' | '}' | '\n' | '\r'))
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Add `cue` (already written as source) to the block: at the start of a spoken line so it
/// begins with the first word, at the end of a heading or an on-screen line (which must
/// keep starting with `[`). Returns `None` for dividers.
pub fn add_cue(src: &str, script: &Script, block: usize, cue: &str) -> Option<String> {
    let (line, at_start) = match script.blocks.get(block)? {
        Block::Heading { line, .. } => (*line, false),
        Block::Line {
            kind: LineKind::OnScreen | LineKind::Note,
            line,
            ..
        } => (*line, false),
        Block::Line { line, .. } => (*line, true),
        Block::Divider { .. } => return None,
    };
    let range = line_range(src, line)?;
    let text = &src[range.clone()];
    let body = text.trim_end_matches(['\n', '\r']);
    let mut out = String::with_capacity(src.len() + cue.len() + 1);
    out.push_str(&src[..range.start]);
    if at_start {
        // After any `LIVE:` / `VO:` prefix, so the line keeps its kind.
        let lead =
            crate::kind_prefix(body).map_or(body.len() - body.trim_start().len(), |(_, n)| n);
        out.push_str(&body[..lead]);
        out.push_str(cue);
        out.push(' ');
        out.push_str(&body[lead..]);
    } else {
        // Before a `{align=center}` paragraph override, which must stay last on the line;
        // everything after the insertion point (override, trailing spaces) is kept as is.
        let at = match crate::attrs::para_attrs(body) {
            Some((r, _)) => r.start,
            None => body.trim_end().len(),
        };
        out.push_str(&body[..at]);
        out.push(' ');
        out.push_str(cue);
        out.push_str(&body[at..]);
    }
    out.push_str(&text[body.len()..]);
    out.push_str(&src[range.end..]);
    Some(out)
}

/// Replace a directive's text (found by its span) with `new`.
pub fn replace_cue(src: &str, old: &Directive, new: &Directive) -> Option<String> {
    let span = old.span.clone();
    src.get(span.clone())?;
    Some(format!(
        "{}{}{}",
        &src[..span.start],
        new.to_source(),
        &src[span.end..]
    ))
}

/// Remove a directive and one space next to it.
pub fn remove_cue(src: &str, d: &Directive) -> Option<String> {
    let mut start = d.span.start;
    let mut end = d.span.end;
    src.get(start..end)?;
    if src[..start].ends_with(' ') {
        start -= 1;
    } else if src[end..].starts_with(' ') {
        end += 1;
    }
    Some(format!("{}{}", &src[..start], &src[end..]))
}

/// Byte range of zero-based line `n`, including its line ending.
fn line_range(src: &str, n: usize) -> Option<std::ops::Range<usize>> {
    let mut start = 0;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        if i == n {
            return Some(start..start + l.len());
        }
        start += l.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const SRC: &str = "---\ntitle: T\n---\n## Intro\n\"Hello there.\"\n[ON SCREEN: title] :sfx[boom]{#s1}\n:transition[cut]{#t1}\n";

    #[test]
    fn round_trips_directives() {
        let s = parse(":music[slow build]{mood=\"very calm\" duck=-8 #m2} :music{mood=hype}");
        let ds: Vec<_> = directives(&s).collect();
        assert_eq!(
            ds[0].to_source(),
            ":music[slow build]{mood=\"very calm\" duck=-8 #m2}"
        );
        assert_eq!(ds[1].to_source(), ":music{mood=hype}");
    }

    #[test]
    fn adds_cues_where_they_belong() {
        let s = parse(SRC);
        let id = next_id(&s, "m");
        assert_eq!(id, "m1");
        let cue = format!(":music[grip]{{#{id}}}");

        let heading = add_cue(SRC, &s, 0, &cue).unwrap();
        assert!(heading.contains("## Intro :music[grip]{#m1}\n"));

        let spoken = add_cue(SRC, &s, 1, &cue).unwrap();
        assert!(spoken.contains("\n:music[grip]{#m1} \"Hello there.\"\n"));
        let reparsed = parse(&spoken);
        assert_eq!(reparsed.blocks.len(), s.blocks.len());

        let on_screen = add_cue(SRC, &s, 2, &cue).unwrap();
        assert!(on_screen.contains("[ON SCREEN: title] :sfx[boom]{#s1} :music[grip]{#m1}\n"));
        assert!(matches!(
            parse(&on_screen).blocks[2],
            Block::Line {
                kind: LineKind::OnScreen,
                ..
            }
        ));

        assert!(add_cue(SRC, &s, 3, &cue).is_none(), "not on dividers");
    }

    #[test]
    fn replaces_and_removes() {
        let src = "## A :music{mood=hype #m1}\nwords here\n";
        let s = parse(src);
        let old = directives(&s).next().unwrap().clone();
        let mut new = old.clone();
        new.label = Some("hype-drop".into());
        let out = replace_cue(src, &old, &new).unwrap();
        assert_eq!(out, "## A :music[hype-drop]{mood=hype #m1}\nwords here\n");

        let s2 = parse(&out);
        let d = directives(&s2).next().unwrap();
        assert_eq!(remove_cue(&out, d).unwrap(), "## A\nwords here\n");
    }

    #[test]
    fn next_id_never_overflows_or_reuses() {
        let s = parse("hello :sfx[old]{#s18446744073709551615}\n");
        assert_eq!(next_id(&s, "s"), "s1");
        let s = parse("hello :sfx[old]{#s4294967295} :sfx[a]{#s1}\n");
        assert_eq!(next_id(&s, "s"), "s4294967296");
        let s = parse("a :sfx[x]{#s18446744073709551615} :sfx[y]{#s1} :sfx[z]{#s2}\n");
        assert_eq!(next_id(&s, "s"), "s3");
    }

    #[test]
    fn appends_before_paragraph_override() {
        for nl in ["\n", "\r\n"] {
            for line in ["## A {align=center}", "[ON SCREEN: title] {align=center}"] {
                let src = format!("{line}{nl}spoken words{nl}");
                let s = parse(&src);
                let out = add_cue(&src, &s, 0, ":look[grain]{#l1}").unwrap();
                let want = line.replace(" {align=center}", " :look[grain]{#l1} {align=center}");
                assert_eq!(out, format!("{want}{nl}spoken words{nl}"));
                // The override is still formatting, not text.
                let p = parse(&out);
                match &p.blocks[0] {
                    Block::Heading { text, cues, .. } => {
                        assert_eq!(text, "A");
                        assert_eq!(cues.len(), 1);
                    }
                    Block::Line { segments, .. } => {
                        assert!(
                            !segments
                                .iter()
                                .any(|g| matches!(g, Segment::Text(t) if t.contains("align")))
                        );
                    }
                    Block::Divider { .. } => panic!(),
                }
            }
        }
        // Trailing spaces stay.
        let src = "## A   \nx\n";
        let out = add_cue(src, &parse(src), 0, ":look[g]{#l1}").unwrap();
        assert_eq!(out, "## A :look[g]{#l1}   \nx\n");
    }
}
