// SPDX-License-Identifier: MIT OR Apache-2.0
//! Execute typed plans through documented FilmCraft commands, then verify readback.
use crate::mcp::Client;
use serde_json::json;
use spearfia_core::{
    CoreError,
    error::Result,
    project::{Project, Timebase},
};
use spearfia_sync::{
    Kind, TimelinePlan,
    render::{Effect, Layer, RenderPlan},
};
use std::collections::BTreeMap;
fn invalid(s: impl Into<String>) -> CoreError {
    CoreError::Invalid(s.into())
}
fn ticks(frame: u64, tb: Timebase) -> i128 {
    frame as i128 * 254_016_000_000i128 * tb.den as i128 / tb.num as i128
}
fn seconds(frames: u64, tb: Timebase) -> f64 {
    frames as f64 * f64::from(tb.den) / f64::from(tb.num)
}
fn text(
    fc: &mut Client,
    project: &Project,
    id: &str,
    start: u64,
    duration: u64,
    layer: &Layer,
) -> Result<u64> {
    let tb = project.file.settings.timebase;
    let [w, h] = project.file.settings.resolution;
    let position = [layer.position[0] * w as f64, layer.position[1] * h as f64];
    let clip = if layer.kind == "media" {
        let path = std::path::absolute(project.resolve(&layer.path))
            .map_err(|e| invalid(e.to_string()))?;
        let imported = fc.tool("media_import", json!({"paths":[path]}))?;
        let item = imported["items"][0]
            .as_u64()
            .ok_or_else(|| invalid("Graphic media could not be imported."))?;
        let probe = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(&path)
            .output()
            .map_err(|e| invalid(e.to_string()))?;
        if !probe.status.success() {
            return Err(invalid("Overlay media cannot be probed."));
        }
        let info: serde_json::Value =
            serde_json::from_slice(&probe.stdout).map_err(|e| invalid(e.to_string()))?;
        let stream = info["streams"]
            .as_array()
            .and_then(|v| v.iter().find(|s| s["codec_type"] == "video"))
            .ok_or_else(|| invalid("Overlay requires picture media."))?;
        if let Some(length) = info["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            && length + seconds(1, tb) < seconds(duration, tb)
        {
            return Err(invalid(
                "Overlay video is shorter than its visual cue. Supply sufficient media or shorten the cue.",
            ));
        }
        let width = stream["width"]
            .as_f64()
            .filter(|v| *v > 0.0)
            .ok_or_else(|| invalid("Overlay has no width."))?;
        let height = stream["height"]
            .as_f64()
            .filter(|v| *v > 0.0)
            .ok_or_else(|| invalid("Overlay has no height."))?;
        let clip=fc.command("timeline.place",json!({"item":item,"track":format!("V{}",layer.track),"frame":start,"duration":ticks(duration,tb),"audio":false}))?["clips"][0].as_u64().ok_or_else(||invalid("Graphic media was not placed."))?;
        for (param, value) in [
            ("position", json!(position)),
            ("uniform_scale", json!(false)),
            ("scale", json!(layer.size[1] * h as f64 / height * 100.0)),
            (
                "scale_width",
                json!(layer.size[0] * w as f64 / width * 100.0),
            ),
        ] {
            fc.command(
                "effects.setParam",
                json!({"clip":clip,"effect":"motion","param":param,"value":value}),
            )?;
        }
        fc.command(
            "effects.setParam",
            json!({"clip":clip,"effect":"opacity","param":"opacity","value":layer.opacity}),
        )?;
        clip
    } else {
        let command = if layer.kind == "shape" {
            "graphics.newShape"
        } else {
            "graphics.newText"
        };
        let result=fc.command(command,json!({"text":layer.text,"shape":layer.shape,"position":position,"size":if layer.kind=="shape"{json!([layer.size[0]*w as f64,layer.size[1]*h as f64])}else{json!(layer.font_size)},"font":layer.font,"track":layer.track-1,"time":ticks(start,tb),"seconds":seconds(duration,tb)}))?;
        let clip = result["clip"]
            .as_u64()
            .ok_or_else(|| invalid("Native graphic creation returned no clip ID."))?;
        let props = if layer.kind == "shape" {
            json!({"fill_color":layer.color,"opacity":layer.opacity})
        } else {
            json!({"fill_color":layer.color,"font":layer.font,"size":layer.font_size,"align":"center","opacity":layer.opacity})
        };
        fc.command("graphics.set", json!({"clip":clip,"layer":0,"props":props}))?;
        clip
    };
    fc.command(
        "clip.rename",
        json!({"clip":clip,"name":format!("{id} {{#{id}}}")}),
    )?;
    Ok(clip)
}
fn effect(
    fc: &mut Client,
    clip: u64,
    start: u64,
    duration: u64,
    tb: Timebase,
    effect: &Effect,
) -> Result<()> {
    if !["motion", "opacity", "volume", "graphic_text"].contains(&effect.effect.as_str()) {
        fc.command(
            "effects.apply",
            json!({"clips":[clip],"effect":effect.effect}),
        )?;
    }
    for (param, value) in &effect.params {
        fc.command(
            "effects.setParam",
            json!({"clip":clip,"effect":effect.effect,"param":param,"value":value}),
        )?;
    }
    for key in &effect.keyframes {
        if key.frame >= duration {
            return Err(invalid("Effect keyframe exceeds its clip scope."));
        }
        fc.command("effects.setParam",json!({"clip":clip,"effect":effect.effect,"param":key.param,"value":key.value,"time":ticks(start+key.frame,tb),"keyframe":true}))?;
    }
    Ok(())
}
pub fn apply(
    fc: &mut Client,
    project: &Project,
    plan: &TimelinePlan,
    render: &RenderPlan,
    clips: &BTreeMap<&str, u64>,
    caption_track: usize,
    words: &[spearfia_analysis::Word],
) -> Result<()> {
    let tb = plan.timebase;
    // Discover the command/effect surface before executing optional features.
    if !render.graphics.is_empty()
        || !render.captions.is_empty()
        || !render.effects.is_empty()
        || render.style.mix.enabled
    {
        let commands = fc.tool("command_list", json!({}))?;
        for needed in [
            "graphics.newText",
            "graphics.set",
            "graphics.setCharStyle",
            "effects.setParam",
            "clipMixer.set",
            "mixer.setKeyframe",
        ] {
            if !commands
                .as_array()
                .is_some_and(|list| list.iter().any(|c| c["id"] == needed))
            {
                return Err(invalid(format!(
                    "Installed FilmCraft does not expose required command {needed}."
                )));
            }
        }
    }
    for bound in &render.effects {
        let clip = *clips
            .get(bound.target.as_str())
            .ok_or_else(|| invalid(format!("Effect target {} was not placed.", bound.target)))?;
        let item = plan
            .items
            .iter()
            .find(|i| i.cue_id == bound.target)
            .unwrap();
        effect(
            fc,
            clip,
            item.start_frame,
            item.duration_frames,
            tb,
            &bound.effect,
        )?;
    }
    let mut expected = Vec::new();
    for graphic in &render.graphics {
        text(
            fc,
            project,
            &graphic.id,
            graphic.start_frame,
            graphic.duration_frames,
            &graphic.layer,
        )?;
        expected.push(graphic.id.clone());
    }
    let c = &render.style.captions;
    for caption in &render.captions {
        let layer = Layer {
            kind: "text".into(),
            text: caption.text.clone(),
            track: caption_track,
            position: c.position,
            font: c.font.clone(),
            font_size: c.size,
            color: c.color.clone(),
            ..Default::default()
        };
        let clip = text(
            fc,
            project,
            &caption.id,
            caption.start_frame,
            caption.duration_frames,
            &layer,
        )?;
        fc.command("graphics.set",json!({"clip":clip,"layer":0,"props":{"font_style":"Bold","stroke":c.outline>0.0,"stroke_width":c.outline,"stroke_color":"#000000"}}))?;
        if caption.active_end > caption.active_start {
            fc.command("graphics.setCharStyle",json!({"clip":clip,"layer":0,"start":caption.active_start,"end":caption.active_end,"style":{"color":c.accent,"bold":true}}))?;
        } else {
            for emphasis in &c.emphasis {
                for (byte, _) in caption.text.match_indices(emphasis) {
                    let start = caption.text[..byte].chars().count();
                    fc.command("graphics.setCharStyle",json!({"clip":clip,"layer":0,"start":start,"end":start+emphasis.chars().count(),"style":{"color":c.accent,"bold":true}}))?;
                }
            }
        }
        if c.pop_ms > 0 && caption.duration_frames > 1 {
            let pop = spearfia_analysis::frame(c.pop_ms as f64 / 1000.0, tb)?
                .max(1)
                .min(caption.duration_frames - 1);
            for (offset, value) in [(0, 92.0), (pop, 100.0)] {
                fc.command("effects.setParam",json!({"clip":clip,"effect":"graphic_text","param":"scale","value":value,"time":ticks(caption.start_frame+offset,tb),"keyframe":true}))?;
            }
        }
        expected.push(caption.id.clone());
    }
    let mix = &render.style.mix;
    if mix.enabled {
        for item in plan
            .items
            .iter()
            .filter(|i| i.kind == Kind::Clip && i.track.starts_with('A'))
        {
            let clip = clips[item.cue_id.as_str()];
            let db = match item.track.as_str() {
                "A1" => mix.voice_db,
                "A2" => mix.music_db,
                "A3" => mix.sfx_db,
                _ => 0.0,
            };
            fc.command(
                "clip.audioGain",
                json!({"clips":[clip],"mode":"set","db":db}),
            )?;
            let fade = spearfia_analysis::frame(mix.fade_seconds, tb)?
                .min(item.duration_frames.saturating_sub(1) / 2);
            if fade > 0 {
                for (frame, value) in [
                    (item.start_frame, -96.0),
                    (item.start_frame + fade, 0.0),
                    (item.start_frame + item.duration_frames - fade - 1, 0.0),
                    (item.start_frame + item.duration_frames - 1, -96.0),
                ] {
                    fc.command("clipMixer.set",json!({"clip":clip,"effect":"volume","param":"level","value":value,"keyframe":true,"time":ticks(frame,tb)}))?;
                }
            }
        }
        // Track automation uses absolute dB; clip gain/fades remain independent.
        fc.command(
            "mixer.setStrip",
            json!({"strip":"A2","volumeDb":0.0,"mode":"Read"}),
        )?;
        let attack = spearfia_analysis::frame(mix.attack_seconds, tb)?;
        let release = spearfia_analysis::frame(mix.release_seconds, tb)?;
        let mut spans: Vec<(u64, u64)> = Vec::new();
        if words.is_empty() {
            for item in &plan.items {
                if item.kind == Kind::Clip && item.track == "A1" {
                    spans.push((item.start_frame, item.start_frame + item.duration_frames));
                }
            }
        } else {
            for word in words {
                if let Some(last) = spans.last_mut()
                    && word.start_frame <= last.1 + attack + release
                {
                    last.1 = last.1.max(word.end_frame);
                    continue;
                }
                spans.push((word.start_frame, word.end_frame));
            }
        }
        let mut envelope = BTreeMap::new();
        envelope.insert(0, 0.0);
        for (start, end) in spans {
            envelope.insert(start.saturating_sub(attack), 0.0);
            envelope.insert(start, mix.duck_db);
            envelope.insert(end, mix.duck_db);
            envelope.insert(end + release, 0.0);
        }
        for (frame, value) in envelope {
            fc.command(
                "mixer.setKeyframe",
                json!({"strip":"A2","lane":"volume","time":ticks(frame,tb),"value":value}),
            )?;
        }
    }
    if !expected.is_empty() {
        let snapshot = fc.tool("sequence_inspect", json!({}))?;
        let names: Vec<_> = snapshot["video"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|track| track["items"].as_array().into_iter().flatten())
            .filter_map(|item| item["name"].as_str())
            .collect();
        for id in expected {
            if !names
                .iter()
                .any(|name| name.contains(&format!("{{#{id}}}")))
            {
                return Err(invalid(format!(
                    "FilmCraft readback did not contain generated graphic {id}."
                )));
            }
        }
    }
    Ok(())
}
