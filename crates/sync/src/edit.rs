//! The edit script (`edit/edit.md`) and the channel's edit rules, resolved to timed operations.
//!
//! An edit script line names where and what, from a fixed vocabulary, so applying it needs no
//! judgement:
//!
//! ```text
//! - section "Round 5" | transition dip-to-black 0.5 | sfx whoosh -8
//! - "he just never stops" | zoom punch-in
//! - "Five for Geralt"@96 | card "FIVE FOR GERALT" 3 | sfx impact
//! ```
//!
//! Where: `section "<heading prefix>"`, or `"<exact script words>"` with an optional `@<line>`
//! hint. Actions: `zoom <recipe>`, `card ["text"] [seconds]`, `transition <dissolve|dip-to-black|
//! dip-to-white> [seconds]`, `sfx <name> [dB]`, `marker "<text>"`. The channel's
//! `spearfia-style` block adds rules applied everywhere they match (`on: "section"` or
//! `on: "generated"` for Visual Script cues made as generated artwork).

use crate::Timing;
use serde::Deserialize;
use spearfia_script::{Block, Script};

#[derive(Debug, Clone, PartialEq)]
pub enum Where {
    Section(String),
    Words { quote: String, line: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Zoom(String),
    /// `None` text: the generated cue's on-screen text.
    Card { text: Option<String>, seconds: Option<f64> },
    Transition { effect: String, seconds: f64 },
    Sfx { name: String, db: Option<f64> },
    Marker(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditLine {
    /// 1-based line in `edit.md`.
    pub line: usize,
    pub at: Where,
    pub actions: Vec<Action>,
}

/// An action and the sequence frame it happens at.
#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub frame: u64,
    pub action: Action,
    /// What asked for it: `edit.md:12`, `rule section`, `rule generated <cue id>`.
    pub from: String,
}

/// The channel's machine-readable edit style (`spearfia-style` block in execution.md).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Style {
    #[serde(default)]
    pub captions: Captions,
    #[serde(default)]
    pub mix: Mix,
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    #[serde(default)]
    pub edit: Rules,
    /// Style settings asked for that the push doesn't apply (`captions.accent`, `mix.duck_db`),
    /// so a report can say so instead of an agent hunting for them in FilmCraft.
    #[serde(skip)]
    pub not_applied: Vec<String>,
}

/// The keys of each style section that the push applies.
const APPLIED: &[(&str, &[&str])] = &[
    ("captions", &["enabled", "font", "size", "color", "outline", "max_chars", "margin", "position"]),
    ("mix", &["enabled", "music_db", "sfx_db", "picture_db"]),
];

/// Set keys (not null, false, zero or empty) the push ignores; disabled sections ask nothing.
fn not_applied(v: &serde_json::Value) -> Vec<String> {
    use serde_json::Value;
    let set = |x: &Value| match x {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        _ => true,
    };
    let mut out = Vec::new();
    for (k, x) in v.as_object().into_iter().flatten() {
        match APPLIED.iter().find(|(s, _)| s == k) {
            Some((_, keys)) if x["enabled"] != Value::Bool(false) => out.extend(
                x.as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(key, val)| !keys.contains(&key.as_str()) && set(val))
                    .map(|(key, _)| format!("{k}.{key}")),
            ),
            Some(_) => {}
            None if !["spec_version", "recipes", "edit"].contains(&k.as_str()) => out.push(k.clone()),
            None => {}
        }
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Captions {
    pub enabled: bool,
    pub font: String,
    pub size: f64,
    pub color: String,
    pub outline: f64,
    pub max_chars: u32,
    /// Bottom margin as a fraction of the frame height.
    pub margin: f64,
    /// `[x, y]` as fractions of the frame; when set, `y` decides the margin (`1 - y`).
    pub position: Option<[f64; 2]>,
}

impl Captions {
    /// The bottom margin FilmCraft gets.
    pub fn bottom_margin(&self) -> f64 {
        self.position.map_or(self.margin, |p| (1.0 - p[1]).clamp(0.0, 0.45))
    }
}

impl Default for Captions {
    fn default() -> Self {
        Self {
            enabled: false,
            font: "Inter".into(),
            size: 64.0,
            color: "#FFFFFF".into(),
            outline: 4.0,
            max_chars: 32,
            margin: 0.1,
            position: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Mix {
    pub enabled: bool,
    pub music_db: f64,
    pub sfx_db: f64,
    /// Picture clips' own sound, carried on A4 at this level for hand mixing. `None` (the
    /// default) leaves picture silent, as clip-based channels want.
    pub picture_db: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub id: String,
    #[serde(default)]
    pub steps: Vec<serde_json::Value>,
}

impl Recipe {
    /// The Motion scale it animates, `(from, to)` percent.
    pub fn scale(&self) -> Option<(f64, f64)> {
        self.steps.iter().find_map(|s| {
            let sc = &s["with"]["scale"];
            Some((sc["from"].as_f64()?, sc["to"].as_f64()?))
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Rules {
    #[serde(default)]
    pub card: CardStyle,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// A title sting: a hold after a section with a background, card and hit.
    #[serde(default)]
    pub intro: Option<Intro>,
    /// A music-only end screen after the last line.
    #[serde(default)]
    pub end_screen: Option<EndScreen>,
}

/// `{title}` in `card` is the video's title in capitals; files are project-relative, an SFX
/// is a name in SFX/ as for `sfx` lines.
#[derive(Debug, Clone, Deserialize)]
pub struct Intro {
    /// Heading prefix of the section it follows.
    pub after: String,
    pub seconds: f64,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub card: Option<String>,
    #[serde(default)]
    pub sfx: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndScreen {
    pub seconds: f64,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub music: Option<String>,
    /// The music plays alone here, so louder than a bed under narration.
    #[serde(default = "end_music_db")]
    pub music_db: f64,
}

fn end_music_db() -> f64 {
    -6.0
}

impl Rules {
    /// The narration-free time the intro and end screen need.
    pub fn holds(&self, tb: spearfia_core::project::Timebase) -> crate::Holds {
        let frames = |s: f64| (s * f64::from(tb.num) / f64::from(tb.den)).round() as u64;
        crate::Holds {
            after: self.intro.iter().map(|i| (i.after.clone(), frames(i.seconds))).collect(),
            end: self.end_screen.as_ref().map_or(0, |e| frames(e.seconds)),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CardStyle {
    pub font: String,
    pub size: f64,
    pub color: String,
    pub outline: f64,
    pub seconds: f64,
}

impl Default for CardStyle {
    fn default() -> Self {
        Self {
            font: "Impact".into(),
            size: 120.0,
            color: "#FFFFFF".into(),
            outline: 8.0,
            seconds: 3.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub on: String,
    #[serde(rename = "do")]
    pub actions: String,
}

/// A Visual Script cue made as generated artwork: where it's anchored and its on-screen text.
pub struct GeneratedCue {
    pub id: String,
    pub quote: String,
    pub line: usize,
    pub text: String,
}

fn style_block(text: &str, name: &str) -> Result<Option<serde_json::Value>, String> {
    let Some(start) = text.find("```spearfia-style") else {
        return Ok(None);
    };
    let body = &text[start..];
    let body = &body[body.find('\n').map_or(body.len(), |n| n + 1)..];
    let end = body.find("```").unwrap_or(body.len());
    serde_json::from_str(&body[..end])
        .map(Some)
        .map_err(|e| format!("{name} spearfia-style block: {e}"))
}

/// The `spearfia-style` JSON block in a channel brief, if any.
pub fn style(brief: &str) -> Result<Option<Style>, String> {
    merged_style(Some(brief), None).map(|(s, found)| found.then_some(s))
}

/// The channel's style with the edit script's own `spearfia-style` block laid over it, key by
/// top-level key, so one video can change its rules without touching the approved channel
/// brief. The flag says whether either had a block.
pub fn merged_style(brief: Option<&str>, edit_md: Option<&str>) -> Result<(Style, bool), String> {
    let base = brief.map(|b| style_block(b, "execution.md")).transpose()?.flatten();
    let over = edit_md.map(|e| style_block(e, "edit.md")).transpose()?.flatten();
    let found = base.is_some() || over.is_some();
    let mut v = base.unwrap_or_else(|| serde_json::json!({}));
    if let (Some(o), Some(map)) = (over.as_ref().and_then(|o| o.as_object()), v.as_object_mut()) {
        for (k, x) in o {
            map.insert(k.clone(), x.clone());
        }
    }
    let skipped = not_applied(&v);
    serde_json::from_value(v)
        .map(|s: Style| (Style { not_applied: skipped, ..s }, found))
        .map_err(|e| format!("spearfia-style: {e}"))
}

/// Split on `sep` outside double quotes.
fn split_quoted(s: &str, sep: char) -> Vec<&str> {
    let (mut out, mut start, mut quoted) = (Vec::new(), 0, false);
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            out.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&s[start..]);
    out.into_iter().map(str::trim).filter(|t| !t.is_empty()).collect()
}

fn unquote(s: &str) -> Option<&str> {
    s.strip_prefix('"')?.strip_suffix('"')
}

/// One action, e.g. `card "ROUND FIVE" 2.5`.
pub fn parse_action(s: &str) -> Result<Action, String> {
    let t = split_quoted(s, ' ');
    let num = |i: usize| -> Result<Option<f64>, String> {
        t.get(i)
            .map(|v| v.parse::<f64>().map_err(|_| format!("`{v}` is not a number in `{s}`")))
            .transpose()
    };
    let text = |i: usize| t.get(i).and_then(|v| unquote(v)).map(str::to_string);
    let too_many = |n: usize| t.len() > n;
    let a = match t.first().copied() {
        Some("zoom") if t.len() == 2 => Action::Zoom(t[1].to_string()),
        Some("card") if !too_many(3) => {
            let (text, at) = match text(1) {
                Some(x) => (Some(x), 2),
                None => (None, 1),
            };
            Action::Card { text, seconds: num(at)? }
        }
        Some("transition") if (2..=3).contains(&t.len()) => {
            let effect = match t[1] {
                "dissolve" | "cross-dissolve" => "cross_dissolve",
                "dip-to-black" => "dip_to_black",
                "dip-to-white" => "dip_to_white",
                other => return Err(format!("unknown transition `{other}` (dissolve, dip-to-black, dip-to-white)")),
            };
            Action::Transition { effect: effect.into(), seconds: num(2)?.unwrap_or(0.5) }
        }
        Some("sfx") if (2..=3).contains(&t.len()) => Action::Sfx { name: t[1].to_string(), db: num(2)? },
        Some("marker") if t.len() == 2 => Action::Marker(text(1).ok_or(format!("marker text needs quotes: `{s}`"))?),
        _ => return Err(format!("unknown action `{s}` (zoom, card, transition, sfx, marker)")),
    };
    Ok(a)
}

fn parse_where(s: &str) -> Result<Where, String> {
    if let Some(rest) = s.strip_prefix("section ") {
        return unquote(rest.trim())
            .map(|t| Where::Section(t.to_string()))
            .ok_or(format!("section name needs quotes: `{s}`"));
    }
    let (q, line) = match s.rsplit_once('@') {
        Some((q, l)) if l.trim().parse::<usize>().is_ok() => (q.trim(), l.trim().parse().unwrap_or(0)),
        _ => (s, 0),
    };
    unquote(q)
        .filter(|q| !q.trim().is_empty())
        .map(|q| Where::Words { quote: q.to_string(), line })
        .ok_or(format!("where must be `section \"…\"` or `\"exact words\"[@line]`: `{s}`"))
}

/// Every `- where | action | …` line of an edit script, and what couldn't be read.
pub fn parse(text: &str) -> (Vec<EditLine>, Vec<String>) {
    let (mut lines, mut problems) = (Vec::new(), Vec::new());
    for (i, raw) in text.lines().enumerate() {
        let Some(body) = raw.trim().strip_prefix("- ") else { continue };
        let parts = split_quoted(body, '|');
        let result = parts
            .first()
            .ok_or_else(|| "empty line".to_string())
            .and_then(|w| parse_where(w))
            .and_then(|at| {
                let actions = parts[1..].iter().map(|a| parse_action(a)).collect::<Result<Vec<_>, _>>()?;
                if actions.is_empty() {
                    return Err("no action".into());
                }
                Ok(EditLine { line: i + 1, at, actions })
            });
        match result {
            Ok(l) => lines.push(l),
            Err(e) => problems.push(format!("edit.md:{}: {e}", i + 1)),
        }
    }
    (lines, problems)
}

/// Every operation with its frame: the edit script's lines plus the channel rules. Problems
/// name what was skipped; nothing is guessed.
pub fn resolve(
    lines: &[EditLine],
    style: &Style,
    generated: &[GeneratedCue],
    timing: &Timing,
    script: &Script,
) -> (Vec<Op>, Vec<String>) {
    let (mut ops, mut problems) = (Vec::new(), Vec::new());
    let sections = timing.sections(script);
    for l in lines {
        let frame = match &l.at {
            Where::Section(p) => sections
                .iter()
                .find(|(t, _)| t.to_lowercase().starts_with(&p.to_lowercase()))
                .map(|s| s.1),
            Where::Words { quote, line } => timing.frame_of(quote, *line),
        };
        let Some(frame) = frame else {
            problems.push(format!("edit.md:{}: {:?} is not in the script", l.line, l.at));
            continue;
        };
        for a in &l.actions {
            if matches!(a, Action::Card { text: None, .. }) {
                problems.push(format!("edit.md:{}: card needs its text here", l.line));
                continue;
            }
            ops.push(Op { frame, action: a.clone(), from: format!("edit.md:{}", l.line) });
        }
    }
    for r in &style.edit.rules {
        let actions: Vec<Action> = match split_quoted(&r.actions, '|').into_iter().map(parse_action).collect() {
            Ok(a) => a,
            Err(e) => {
                problems.push(format!("rule on {}: {e}", r.on));
                continue;
            }
        };
        let targets: Vec<(u64, Option<&GeneratedCue>)> = match r.on.as_str() {
            // The first section opens the video: nothing to transition from.
            "section" => sections.iter().skip(1).map(|s| (s.1, None)).collect(),
            "generated" => generated
                .iter()
                .filter_map(|c| match timing.frame_of(&c.quote, c.line) {
                    Some(f) => Some((f, Some(c))),
                    None => {
                        problems.push(format!("generated cue {} is not in the script", c.id));
                        None
                    }
                })
                .collect(),
            other => {
                problems.push(format!("rule on `{other}`: use section or generated"));
                continue;
            }
        };
        for (frame, cue) in targets {
            for a in &actions {
                let action = match (a, cue) {
                    (Action::Card { text: None, seconds }, Some(c)) if !c.text.trim().is_empty() => {
                        Action::Card { text: Some(c.text.clone()), seconds: *seconds }
                    }
                    (Action::Card { text: None, .. }, _) => continue,
                    _ => a.clone(),
                };
                let from = cue.map_or(format!("rule {}", r.on), |c| format!("rule {} {}", r.on, c.id));
                ops.push(Op { frame, action, from });
            }
        }
    }
    ops.sort_by_key(|o| o.frame);
    (ops, problems)
}

/// The on-screen text of a Visual Script cue body: its `TEXT:` lines, joined by newlines.
pub fn cue_text(body: &str) -> String {
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("TEXT:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `## ` headings of the script in order, for summaries.
pub fn headings(script: &Script) -> Vec<String> {
    script
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { level: 2, text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Heard, Timing};
    use spearfia_core::links::{Link, LinkKind, LinkMedia, Links};
    use spearfia_core::project::{MediaRole, Timebase};
    use std::collections::BTreeMap;

    const SCRIPT: &str = "## Cold Open\nVO: one two three four\n## Round 1\nVO: five six seven eight\n";

    #[test]
    fn edit_lines_parse_and_report_problems() {
        let (lines, problems) = parse(
            "# Edit\n- section \"Round 1\" | transition dip-to-black 0.5 | sfx whoosh -8\n\
             - \"six seven\"@4 | zoom punch-in | card \"SIX | SEVEN\" 2\n\
             - \"six\" | explode\n- nowhere | zoom punch-in\n",
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1].actions,
            [Action::Zoom("punch-in".into()), Action::Card { text: Some("SIX | SEVEN".into()), seconds: Some(2.0) }]
        );
        assert_eq!(lines[1].at, Where::Words { quote: "six seven".into(), line: 4 });
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("edit.md:4: unknown action"));
    }

    #[test]
    fn lines_and_rules_resolve_to_frames() {
        let script = spearfia_script::parse(SCRIPT);
        let vo = Link {
            id: "k1".into(),
            quote: "one two three four".into(),
            line: 2,
            media: LinkMedia { path: "vo.wav".into(), hash: None },
            role: MediaRole::Voiceover,
            kind: LinkKind::for_role(MediaRole::Voiceover),
            in_point: None,
            out_point: None,
        };
        let links = Links { links: vec![vo], next_id: 2, unreadable: vec![] };
        let media = BTreeMap::from([("vo.wav".to_string(), Some(240))]);
        let timing = Timing::new(&script, &links, &media, Timebase::FPS_30, &Heard::new(), &crate::Holds::default());
        let style: Style = serde_json::from_value(serde_json::json!({
            "edit": {"rules": [{"on": "section", "do": "transition dip-to-black 0.5"},
                               {"on": "generated", "do": "card | sfx impact"}]}
        }))
        .unwrap();
        let (lines, _) = parse("- \"seven\" | zoom punch-in\n- \"missing words\" | zoom punch-in\n");
        let generated = [GeneratedCue { id: "c1".into(), quote: "six".into(), line: 4, text: "SIX".into() }];
        let (ops, problems) = resolve(&lines, &style, &generated, &timing, &script);
        // 8 words over 240 frames: "five" (Round 1) at 120, "six" at 150, "seven" at 180.
        let got: Vec<(u64, &Action)> = ops.iter().map(|o| (o.frame, &o.action)).collect();
        assert_eq!(
            got,
            [
                (120, &Action::Transition { effect: "dip_to_black".into(), seconds: 0.5 }),
                (150, &Action::Card { text: Some("SIX".into()), seconds: None }),
                (150, &Action::Sfx { name: "impact".into(), db: None }),
                (180, &Action::Zoom("punch-in".into())),
            ]
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn style_block_and_cue_text_are_read() {
        let brief = "x\n```spearfia-style\n{\"captions\": {\"enabled\": true, \"font\": \"Impact\"},\n\
                     \"recipes\": [{\"id\": \"c/punch-in@1\", \"steps\": [{\"with\": {\"scale\": {\"from\": 100, \"to\": 118}}}]}]}\n```\n";
        let s = style(brief).unwrap().unwrap();
        assert!(s.captions.enabled && s.captions.font == "Impact" && s.captions.max_chars == 32);
        assert_eq!(s.captions.bottom_margin(), 0.1);
        assert_eq!(s.recipes[0].scale(), Some((100.0, 118.0)));
        assert_eq!(cue_text("TEXT: FIVE\nGRAPHIC: x\nTEXT: FOR GERALT\n"), "FIVE\nFOR GERALT");
        assert!(style("no block").unwrap().is_none());
        let edit = "```spearfia-style\n{\"edit\": {\"rules\": [{\"on\": \"section\", \"do\": \"marker \\\"x\\\"\"}]}}\n```";
        let (m, found) = merged_style(Some(brief), Some(edit)).unwrap();
        assert!(found && m.captions.enabled && m.edit.rules.len() == 1);
        // Picture sound stays muted unless a channel asks for it.
        assert_eq!(s.mix.picture_db, None);
        let asmr = "```spearfia-style
{\"mix\": {\"enabled\": true, \"picture_db\": -8}}
```";
        assert_eq!(style(asmr).unwrap().unwrap().mix.picture_db, Some(-8.0));
    }

    #[test]
    fn settings_the_push_ignores_are_reported() {
        let brief = "```spearfia-style
{\"spec_version\": 1, \"music\": \"x\",
 \"captions\": {\"enabled\": true, \"font\": \"Impact\", \"accent\": \"#E3262B\", \"emphasis\": [], \"pop_ms\": 120},
 \"mix\": {\"enabled\": true, \"music_db\": -16, \"voice_db\": 0, \"duck_db\": -6}}
```";
        assert_eq!(style(brief).unwrap().unwrap().not_applied, ["captions.accent", "captions.pop_ms", "mix.duck_db", "music"]);
        let off = "```spearfia-style\n{\"mix\": {\"enabled\": false, \"duck_db\": -6}}\n```";
        assert!(style(off).unwrap().unwrap().not_applied.is_empty());
    }
}
