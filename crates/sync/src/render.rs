// SPDX-License-Identifier: MIT OR Apache-2.0
//! Executable caption, graphic, effect and audio treatments from channel/project intent.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spearfia_analysis::Word;
use spearfia_core::{CoreError, error::Result, project::Project, visuals};
use std::collections::BTreeMap;
fn invalid(s: impl Into<String>) -> CoreError {
    CoreError::Invalid(s.into())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionStyle {
    pub enabled: bool,
    pub font: String,
    pub size: f64,
    pub color: String,
    pub accent: String,
    pub position: [f64; 2],
    pub max_words: usize,
    pub words_per_line: usize,
    pub max_chars: usize,
    pub max_gap_ms: u64,
    pub pop_ms: u64,
    pub mode: String,
    pub emphasis: Vec<String>,
    pub outline: f64,
}
impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            enabled: false,
            font: "Arial".into(),
            size: 64.0,
            color: "#ffffff".into(),
            accent: "#ffcc00".into(),
            position: [0.5, 0.85],
            max_words: 6,
            words_per_line: 3,
            max_chars: 42,
            max_gap_ms: 500,
            pop_ms: 100,
            mode: "group".into(),
            emphasis: vec![],
            outline: 3.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MixStyle {
    pub enabled: bool,
    pub voice_db: f64,
    pub music_db: f64,
    pub sfx_db: f64,
    pub fade_seconds: f64,
    pub duck_db: f64,
    pub attack_seconds: f64,
    pub release_seconds: f64,
}
impl Default for MixStyle {
    fn default() -> Self {
        Self {
            enabled: false,
            voice_db: 0.0,
            music_db: -20.0,
            sfx_db: -10.0,
            fade_seconds: 0.1,
            duck_db: -8.0,
            attack_seconds: 0.1,
            release_seconds: 0.25,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub requires: Vec<String>,
    pub steps: Vec<RecipeStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeStep {
    #[serde(rename = "use")]
    pub capability: String,
    #[serde(default)]
    pub target: String,
    #[serde(rename = "with", default)]
    pub params: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub spec_version: u32,
    pub captions: CaptionStyle,
    pub mix: MixStyle,
    pub recipes: Vec<Recipe>,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            spec_version: 1,
            captions: Default::default(),
            mix: Default::default(),
            recipes: vec![],
        }
    }
}
impl Style {
    pub fn validate(&self) -> Result<()> {
        let c = &self.captions;
        let m = &self.mix;
        if self.spec_version != 1
            || !(1..=30).contains(&c.max_words)
            || !(1..=c.max_words).contains(&c.words_per_line)
            || !(1..=300).contains(&c.max_chars)
            || !["group", "word"].contains(&c.mode.as_str())
            || !c.size.is_finite()
            || !(1.0..=1000.0).contains(&c.size)
            || c.position
                .iter()
                .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
            || !c.outline.is_finite()
            || c.outline < 0.0
            || c.font.is_empty()
            || c.emphasis.len() > 1000
        {
            return Err(invalid("Invalid caption profile values."));
        }
        for color in [&c.color, &c.accent] {
            if !valid_color(color) {
                return Err(invalid("Caption colors must be #RRGGBB or #RRGGBBAA."));
            }
        }
        if [m.voice_db, m.music_db, m.sfx_db, m.duck_db]
            .iter()
            .any(|v| !v.is_finite() || !(-96.0..=24.0).contains(v))
            || [m.fade_seconds, m.attack_seconds, m.release_seconds]
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=10.0).contains(v))
        {
            return Err(invalid("Invalid mix dB levels or fade/duck durations."));
        }
        let mut ids = std::collections::BTreeSet::new();
        for recipe in &self.recipes {
            if recipe.steps.is_empty() || recipe.steps.len() > 100 || !ids.insert(&recipe.id) {
                return Err(invalid("Recipes need unique IDs and 1–100 steps."));
            }
        }
        Ok(())
    }
    pub fn parse(source: &str) -> Result<Self> {
        let style: Self = serde_json::from_str(source)
            .map_err(|e| invalid(format!("Invalid execution style: {e}")))?;
        style.validate()?;
        Ok(style)
    }
}
pub fn valid_color(s: &str) -> bool {
    [7, 9].contains(&s.len()) && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn load_style(project: &Project) -> Result<Style> {
    let path = project.root.join("production/style.json");
    if path.exists() {
        return Style::parse(&spearfia_core::channels::read_limited(&path, 1024 * 1024)?);
    }
    let brief = project.channel_brief()?.unwrap_or_default();
    let mut blocks = brief.split("```spearfia-style").skip(1);
    let Some(block) = blocks.next() else {
        return Ok(Style::default());
    };
    if blocks.next().is_some() {
        return Err(invalid(
            "The channel brief contains multiple executable style blocks; choose one project style.",
        ));
    }
    Style::parse(block.split("```").next().unwrap_or_default().trim())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub param: String,
    pub frame: u64,
    pub value: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub effect: String,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub kind: String,
    pub text: String,
    pub path: String,
    pub track: usize,
    pub position: [f64; 2],
    pub size: [f64; 2],
    pub font: String,
    pub font_size: f64,
    pub color: String,
    pub shape: String,
    pub opacity: f64,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            kind: "text".into(),
            text: String::new(),
            path: String::new(),
            track: 4,
            position: [0.5, 0.5],
            size: [0.5, 0.2],
            font: "Arial".into(),
            font_size: 64.0,
            color: "#ffffff".into(),
            shape: "rectangle".into(),
            opacity: 100.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub from: f64,
    pub to: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Treatment {
    pub layers: Vec<Layer>,
    pub effects: Vec<Effect>,
    pub recipes: Vec<String>,
    pub camera: Option<Camera>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graphic {
    pub id: String,
    pub start_frame: u64,
    pub duration_frames: u64,
    pub layer: Layer,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundEffect {
    pub target: String,
    pub effect: Effect,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caption {
    pub id: String,
    pub start_frame: u64,
    pub duration_frames: u64,
    pub text: String,
    pub active_start: usize,
    pub active_end: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPlan {
    pub spec_version: u32,
    pub graphics: Vec<Graphic>,
    pub effects: Vec<BoundEffect>,
    pub captions: Vec<Caption>,
    pub style: Style,
}
pub fn captions(
    words: &[Word],
    style: &CaptionStyle,
    tb: spearfia_core::project::Timebase,
) -> Result<Vec<Caption>> {
    if !style.enabled {
        return Ok(vec![]);
    }
    let gap = spearfia_analysis::frame(style.max_gap_ms as f64 / 1000.0, tb)?;
    let mut groups: Vec<Vec<&Word>> = Vec::new();
    let mut group = Vec::new();
    let mut chars = 0;
    for word in words {
        let previous = group.last().copied();
        let boundary = previous.is_some_and(|p: &Word| {
            word.line != p.line
                || word.start_frame.saturating_sub(p.end_frame) > gap
                || p.text.ends_with(['.', '!', '?', ';', ':'])
        });
        if !group.is_empty()
            && (boundary
                || group.len() >= style.max_words
                || chars + word.text.chars().count() + 1 > style.max_chars)
        {
            groups.push(std::mem::take(&mut group));
            chars = 0;
        }
        chars += word.text.chars().count() + 1;
        group.push(word);
    }
    if !group.is_empty() {
        groups.push(group);
    }
    let mut result = Vec::new();
    for (g, words) in groups.iter().enumerate() {
        let mut text = String::new();
        let mut ranges = Vec::new();
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                text.push(if i % style.words_per_line == 0 {
                    '\n'
                } else {
                    ' '
                });
            }
            let start = text.chars().count();
            text.push_str(&w.text);
            ranges.push((start, text.chars().count()));
        }
        if style.mode == "word" {
            for (i, w) in words.iter().enumerate() {
                let end = words
                    .get(i + 1)
                    .map_or(w.end_frame, |next| next.start_frame);
                if end <= w.start_frame {
                    return Err(invalid("Caption words have non-monotonic frame times."));
                }
                result.push(Caption {
                    id: format!("caption-{g}-{i}"),
                    start_frame: w.start_frame,
                    duration_frames: end - w.start_frame,
                    text: text.clone(),
                    active_start: ranges[i].0,
                    active_end: ranges[i].1,
                });
            }
        } else {
            let first = words[0];
            let last = words.last().unwrap();
            result.push(Caption {
                id: format!("caption-{g}"),
                start_frame: first.start_frame,
                duration_frames: last.end_frame - first.start_frame,
                text,
                active_start: 0,
                active_end: 0,
            });
        }
    }
    Ok(result)
}
fn recipe_effects(style: &Style, ids: &[String], duration: u64) -> Result<Vec<Effect>> {
    let mut effects = Vec::new();
    for id in ids {
        let recipe = style.recipes.iter().find(|r| &r.id == id).ok_or_else(|| {
            invalid(format!(
                "Recipe {id} is not defined in the execution style."
            ))
        })?;
        if !recipe.requires.is_empty() {
            return Err(invalid(format!(
                "Recipe {id} requires unavailable analysis: {:?}",
                recipe.requires
            )));
        }
        for step in &recipe.steps {
            let effect=step.capability.strip_prefix("filmcraft/").and_then(|s|s.strip_suffix("#apply")).ok_or_else(||invalid(format!("Capability {} has no verified FilmCraft native mapping. A plugin source file alone is not an installed effect.",step.capability)))?;
            if !step.target.is_empty() && step.target != "picture" {
                return Err(invalid(
                    "Native recipe steps currently target picture clips explicitly.",
                ));
            }
            let mut params = BTreeMap::new();
            let mut keyframes = Vec::new();
            for (key, value) in &step.params {
                if let (Some(from), Some(to)) = (value.get("from"), value.get("to")) {
                    keyframes.push(Keyframe {
                        param: key.clone(),
                        frame: 0,
                        value: from.clone(),
                    });
                    keyframes.push(Keyframe {
                        param: key.clone(),
                        frame: duration.saturating_sub(1),
                        value: to.clone(),
                    });
                } else {
                    params.insert(key.clone(), value.clone());
                }
            }
            effects.push(Effect {
                effect: effect.into(),
                params,
                keyframes,
            });
        }
    }
    Ok(effects)
}
/// Interpret only typed execution metadata. Production prose remains creator intent,
/// and is never passed to an arbitrary editor command evaluator.
pub fn compile(
    project: &Project,
    plan: &mut super::TimelinePlan,
    words: &[Word],
) -> Result<RenderPlan> {
    let style = load_style(project)?;
    let source = project.read_script()?;
    if needs_timing(project)? {
        let expected = spearfia_analysis::script_words(&source);
        if expected
            .iter()
            .any(|w| !words.iter().any(|a| a.line == w.line && a.word == w.word))
        {
            return Err(invalid(
                "Executable treatments require measured timing for every narration word.",
            ));
        }
    }
    let mut render = RenderPlan {
        spec_version: 1,
        graphics: vec![],
        effects: vec![],
        captions: captions(words, &style.captions, plan.timebase)?,
        style,
    };
    let visual = visuals::Plan::parse(&project.read_visuals()?)?;
    let mut treatments = Vec::new();
    let mut boundaries = std::collections::BTreeSet::new();
    for cue in visual.cues {
        let Some(raw) = cue.meta.details.get("execution") else {
            let body = &visual.source[cue.body.clone()];
            if body.lines().any(|line| {
                matches!(
                    visuals::Role::of(line),
                    Some(visuals::Role::Text | visuals::Role::Graphic | visuals::Role::Action)
                )
            }) || ["generate", "comfyui", "graphic", "chart", "animation"]
                .iter()
                .any(|kind| {
                    cue.meta.method.to_lowercase().contains(kind)
                        || cue.meta.visual_type.to_lowercase().contains(kind)
                })
            {
                return Err(invalid(format!(
                    "Visual cue {} contains production instructions without executable treatment metadata. Resolve its graphics/actions before assembly.",
                    cue.meta.id
                )));
            }
            continue;
        };
        let treatment: Treatment = serde_json::from_str(raw).map_err(|e| {
            invalid(format!(
                "Visual cue {} has invalid execution JSON: {e}",
                cue.meta.id
            ))
        })?;
        let range = cue.meta.anchor.resolve(&source).range.ok_or_else(|| {
            invalid(format!(
                "Visual cue {} has a stale narration attachment.",
                cue.meta.id
            ))
        })?;
        let (start, end) = spearfia_analysis::range_frames(&source, range, words)?;
        if end <= start {
            return Err(invalid("Visual treatment has no measured duration."));
        }
        boundaries.insert(start);
        boundaries.insert(end);
        treatments.push((cue.meta.id, treatment, start, end));
    }
    let script = spearfia_script::parse(&source);
    for look in spearfia_script::looks::looks(&script, &spearfia_script::timing::timeline(&script))
    {
        if look.covers.is_empty() {
            return Err(invalid("A look cue covers no timed narration."));
        }
        let lines: std::collections::BTreeSet<_> = look
            .covers
            .iter()
            .map(|i| spearfia_script::looks::line_of(&script.blocks[*i]))
            .collect();
        let measured: Vec<_> = words.iter().filter(|w| lines.contains(&w.line)).collect();
        let start = measured
            .iter()
            .map(|w| w.start_frame)
            .min()
            .ok_or_else(|| invalid("Look cues require measured narration timing."))?;
        let end = measured.iter().map(|w| w.end_frame).max().unwrap();
        if look.params().next().is_some() {
            return Err(invalid(format!(
                "Look {} has parameters not represented by its saved recipe. Put executable parameters in the channel recipe or visual treatment.",
                look.label()
            )));
        }
        let label = look.label();
        let recipe = render
            .style
            .recipes
            .iter()
            .find(|r| r.id == label || r.name == label)
            .ok_or_else(|| invalid(format!("Look {label} has no executable channel recipe.")))?;
        boundaries.insert(start);
        boundaries.insert(end);
        treatments.push((
            format!("look-{}", look.block),
            Treatment {
                recipes: vec![recipe.id.clone()],
                ..Default::default()
            },
            start,
            end,
        ));
    }
    // Split pictures at cue boundaries so a short treatment cannot leak across its cue.
    let mut items = Vec::new();
    let mut first_parts = BTreeMap::new();
    for item in &plan.items {
        if item.kind != super::Kind::Clip || !item.track.starts_with('V') {
            items.push(item.clone());
            continue;
        }
        let end = item.start_frame + item.duration_frames;
        let mut points = vec![item.start_frame];
        points.extend(
            boundaries
                .range((
                    std::ops::Bound::Excluded(item.start_frame),
                    std::ops::Bound::Excluded(end),
                ))
                .copied(),
        );
        points.push(end);
        for (i, pair) in points.windows(2).enumerate() {
            let mut clip = item.clone();
            if points.len() > 2 {
                clip.cue_id = format!("{}-part-{i}", item.cue_id);
                if i == 0 {
                    first_parts.insert(item.cue_id.clone(), clip.cue_id.clone());
                }
            }
            clip.start_frame = pair[0];
            clip.duration_frames = pair[1] - pair[0];
            if let Some(src) = &mut clip.source {
                src.in_frame += pair[0] - item.start_frame;
            }
            let params = clip.params.get_or_insert_with(|| json!({}));
            params["source_link"] = json!(item.cue_id);
            items.push(clip);
        }
    }
    plan.items = items;
    for item in &mut plan.items {
        if item.kind == super::Kind::Transition
            && let Some(params) = &mut item.params
            && let Some(attach) = params["attach"].as_str()
            && let Some(first) = first_parts.get(attach)
        {
            params["attach"] = json!(first);
        }
    }
    for (id, treatment, start, end) in treatments {
        if treatment.layers.len() > 100 || treatment.effects.len() > 100 {
            return Err(invalid("Visual cue exceeds execution limits."));
        }
        for (i, layer) in treatment.layers.into_iter().enumerate() {
            if !["text", "shape", "media"].contains(&layer.kind.as_str())
                || !(4..=16).contains(&layer.track)
                || !valid_color(&layer.color)
                || layer
                    .position
                    .iter()
                    .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
                || layer.size.iter().any(|p| !p.is_finite() || *p <= 0.0)
                || !layer.font_size.is_finite()
                || layer.font_size <= 0.0
                || !layer.opacity.is_finite()
                || !(0.0..=100.0).contains(&layer.opacity)
            {
                return Err(invalid(format!("Invalid executable layer in cue {id}.")));
            }
            if layer.kind == "media" && !project.resolve(&layer.path).is_file() {
                return Err(invalid(format!(
                    "Cue {id} requires missing graphic media {}.",
                    layer.path
                )));
            }
            if layer.kind == "text" && layer.text.trim().is_empty() {
                return Err(invalid(format!("Cue {id} has empty on-screen text.")));
            }
            if layer.kind == "shape" && !["rectangle", "ellipse"].contains(&layer.shape.as_str()) {
                return Err(invalid("Unsupported graphic shape."));
            }
            render.graphics.push(Graphic {
                id: format!("{id}-layer-{i}"),
                start_frame: start,
                duration_frames: end - start,
                layer,
            });
        }
        let mut effects = treatment.effects;
        effects.extend(recipe_effects(
            &render.style,
            &treatment.recipes,
            end - start,
        )?);
        if let Some(camera) = treatment.camera {
            if !camera.from.is_finite()
                || !camera.to.is_finite()
                || camera.from <= 0.0
                || camera.to <= 0.0
            {
                return Err(invalid("Camera zoom values must be positive percentages."));
            }
            effects.push(Effect {
                effect: "motion".into(),
                params: BTreeMap::new(),
                keyframes: vec![
                    Keyframe {
                        param: "scale".into(),
                        frame: 0,
                        value: json!(camera.from),
                    },
                    Keyframe {
                        param: "scale".into(),
                        frame: end - start - 1,
                        value: json!(camera.to),
                    },
                ],
            });
        }
        for item in &plan.items {
            if item.kind == super::Kind::Clip
                && item.track.starts_with('V')
                && item.start_frame >= start
                && item.start_frame < end
            {
                for effect in &effects {
                    let effect = slice_effect(
                        effect,
                        item.start_frame - start,
                        item.duration_frames,
                        end - start,
                    )?;
                    render.effects.push(BoundEffect {
                        target: item.cue_id.clone(),
                        effect,
                    });
                }
            }
        }
    }
    let mut assigned = std::collections::BTreeSet::new();
    for bound in &render.effects {
        let params: std::collections::BTreeSet<_> = bound
            .effect
            .params
            .keys()
            .chain(bound.effect.keyframes.iter().map(|k| &k.param))
            .collect();
        for param in params {
            if !assigned.insert((&bound.target, &bound.effect.effect, param)) {
                return Err(invalid(format!(
                    "Overlapping treatments set {}.{} twice on {}. Combine them into one reviewed curve.",
                    bound.effect.effect, param, bound.target
                )));
            }
        }
    }
    Ok(render)
}
pub fn needs_timing(project: &Project) -> Result<bool> {
    let script = spearfia_script::parse(&project.read_script()?);
    Ok(
        !spearfia_script::looks::looks(&script, &spearfia_script::timing::timeline(&script))
            .is_empty()
            || load_style(project)?.captions.enabled
            || load_style(project)?.mix.enabled
            || visuals::Plan::parse(&project.read_visuals()?)?
                .cues
                .iter()
                .any(|c| c.meta.details.contains_key("execution")),
    )
}

fn slice_effect(effect: &Effect, offset: u64, duration: u64, total: u64) -> Result<Effect> {
    if effect.effect.trim().is_empty() {
        return Err(invalid("An executable effect requires a native effect ID."));
    }
    let mut result = effect.clone();
    result.keyframes.clear();
    let mut curves: BTreeMap<&str, Vec<&Keyframe>> = BTreeMap::new();
    for key in &effect.keyframes {
        if key.frame >= total {
            return Err(invalid("Effect keyframe exceeds visual cue duration."));
        }
        curves.entry(&key.param).or_default().push(key);
    }
    for (param, mut curve) in curves {
        curve.sort_by_key(|k| k.frame);
        if curve.windows(2).any(|p| p[0].frame == p[1].frame) {
            return Err(invalid("Effect curve has duplicate frame positions."));
        }
        let value = |at: u64| -> Result<Value> {
            let before = curve
                .iter()
                .rev()
                .find(|k| k.frame <= at)
                .copied()
                .unwrap_or(curve[0]);
            let after = curve
                .iter()
                .find(|k| k.frame >= at)
                .copied()
                .unwrap_or(curve[curve.len() - 1]);
            if before.frame == after.frame || before.value == after.value {
                return Ok(before.value.clone());
            }
            match (before.value.as_f64(), after.value.as_f64()) {
                (Some(a), Some(b)) => Ok(json!(
                    a + (b - a) * (at - before.frame) as f64 / (after.frame - before.frame) as f64
                )),
                _ => Err(invalid(
                    "Animated recipe values must be numbers when a cue spans several clips.",
                )),
            }
        };
        let last = offset + duration.saturating_sub(1);
        let mut frames = std::collections::BTreeSet::from([offset, last]);
        frames.extend(
            curve
                .iter()
                .filter(|k| k.frame > offset && k.frame < last)
                .map(|k| k.frame),
        );
        for frame in frames {
            result.keyframes.push(Keyframe {
                param: param.into(),
                frame: frame - offset,
                value: value(frame)?,
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn word(line: usize, word: usize, text: &str, start: u64, end: u64) -> Word {
        Word {
            line,
            word,
            text: text.into(),
            start_frame: start,
            end_frame: end,
            estimated: false,
        }
    }
    #[test]
    fn captions_group_at_pauses_and_keep_unicode_character_ranges() {
        let mut style = Style::default();
        style.captions.enabled = true;
        style.captions.mode = "word".into();
        style.captions.words_per_line = 2;
        let words = vec![
            word(2, 0, "Café", 3, 6),
            word(2, 1, "is", 7, 9),
            word(2, 2, "open.", 10, 14),
            word(2, 3, "Again", 40, 45),
        ];
        let result = captions(
            &words,
            &style.captions,
            spearfia_core::project::Timebase { num: 30, den: 1 },
        )
        .unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[1].text, "Café is\nopen.");
        assert_eq!((result[1].active_start, result[1].active_end), (5, 7));
        assert_eq!(result[0].duration_frames, 4);
        assert_eq!(result[3].text, "Again");
    }
    #[test]
    fn splitting_preserves_the_original_animation_curve() {
        let effect = Effect {
            effect: "motion".into(),
            params: BTreeMap::new(),
            keyframes: vec![
                Keyframe {
                    param: "scale".into(),
                    frame: 0,
                    value: json!(100),
                },
                Keyframe {
                    param: "scale".into(),
                    frame: 99,
                    value: json!(199),
                },
            ],
        };
        let part = slice_effect(&effect, 30, 20, 100).unwrap();
        assert_eq!(part.keyframes[0].value, json!(130.0));
        assert_eq!(part.keyframes[1].value, json!(149.0));
        assert_eq!(part.keyframes[1].frame, 19);
    }
    #[test]
    fn unsupported_capability_is_an_actionable_error() {
        let style = Style {
            recipes: vec![Recipe {
                id: "loupe".into(),
                name: "Loupe".into(),
                requires: vec![],
                steps: vec![RecipeStep {
                    capability: "spearfia/focus-loupe@0.1.0#apply".into(),
                    target: "picture".into(),
                    params: BTreeMap::new(),
                }],
            }],
            ..Default::default()
        };
        assert!(
            recipe_effects(&style, &["loupe".into()], 30)
                .unwrap_err()
                .to_string()
                .contains("no verified FilmCraft native mapping")
        );
    }
}
