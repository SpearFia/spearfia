// SPDX-License-Identifier: MIT OR Apache-2.0
use super::invalid;
use crate::{Origin, Result};
use spearfia_core::{
    links::Links,
    project::{Project, write_json},
};
pub use spearfia_sync::render::{Style, load_style};
pub const FILE: &str = "production/style.json";
pub fn save(project: &Project, style: &Style, expected: &str, origin: Origin) -> Result<String> {
    style.validate()?;
    let doc = crate::Doc::from_text(FILE, crate::read(project, FILE)?);
    doc.expect(Some(expected))?;
    crate::apply(
        project,
        &doc.plan(
            serde_json::to_string_pretty(style).map_err(|e| invalid(e.to_string()))? + "\n",
            "Configure executable production style",
        ),
        origin,
    )
}
pub fn build_captions(project: &Project) -> Result<serde_json::Value> {
    project.approved_script()?;
    let style = load_style(project)?;
    if !style.captions.enabled {
        return Err(invalid(
            "Enable and review a channel or project caption profile before building captions.",
        ));
    }
    let links = Links::load(project)?;
    let words = spearfia_analysis::timeline_words(project, &links, project.file.settings.timebase)?;
    let source = project.read_script()?;
    if spearfia_analysis::script_words(&source)
        .iter()
        .any(|w| !words.iter().any(|a| a.line == w.line && a.word == w.word))
    {
        return Err(invalid(
            "Caption generation requires measured timing for every spoken word.",
        ));
    }
    let captions =
        spearfia_sync::render::captions(&words, &style.captions, project.file.settings.timebase)?;
    let output = serde_json::json!({"script_revision":spearfia_core::version(&source),"timebase":project.file.settings.timebase,"style":style.captions,"captions":captions});
    write_json(&project.root.join("cache/captions.json"), &output)?;
    Ok(output)
}
