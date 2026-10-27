//! Reusable channel knowledge, kept as ordinary Markdown outside video projects.
use crate::error::{CoreError, Result};
use crate::project::{Project, slug, write_text};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
pub const MAX_BRIEF_BYTES: usize = 2 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 32;
const MAX_CHANNELS: usize = 200;
pub const PROJECT_BRIEF: &str = "channel.md";
const APPROVED_SCRIPT: &str = "sync/approved-script.md";
const APPROVED_CHANNEL: &str = "sync/approved-channel.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub title: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub name: String,
    pub text: String,
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !["con", "prn", "aux", "nul"].contains(&id)
        && !(1..=9).any(|i| id == format!("com{i}") || id == format!("lpt{i}"))
}

fn document_path(root: &Path, name: &str) -> Result<PathBuf> {
    let stem = name
        .strip_suffix(".md")
        .ok_or_else(|| invalid("Choose a Markdown (.md) document."))?;
    if stem.is_empty()
        || name.len() > 120
        || stem.starts_with('.')
        || stem.ends_with(['.', ' '])
        || !stem
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.'))
        || !id_ok(&slug(stem.split('.').next().unwrap_or(stem)))
    {
        return Err(invalid(
            "Use a Markdown filename without path separators or reserved characters.",
        ));
    }
    let path = root.join(name);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(invalid(
            "Channel documents must be regular Markdown files, not symbolic links.",
        ));
    }
    Ok(path)
}

pub fn read_limited(path: &Path, max: usize) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)
        .map_err(|e| CoreError::io(path, e))?
        .take(max.saturating_add(1) as u64)
        .read_to_string(&mut text)
        .map_err(|e| CoreError::io(path, e))?;
    if text.len() > max {
        return Err(invalid(format!(
            "{} exceeds the {} KiB text limit.",
            path.display(),
            max / 1024
        )));
    }
    Ok(text)
}

fn title(text: &str, fallback: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .chars()
        .take(120)
        .collect()
}

pub fn list(config: &Path) -> Result<Vec<Channel>> {
    let root = config.join("channels");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(CoreError::io(&root, e)),
    };
    let mut channels = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(&root, e))?;
        if !entry
            .file_type()
            .map_err(|e| CoreError::io(entry.path(), e))?
            .is_dir()
        {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !id_ok(&id) {
            continue;
        }
        if channels.len() == MAX_CHANNELS {
            return Err(invalid("The channel library is limited to 200 channels."));
        }
        channels.push(load(config, &id)?);
    }
    channels.sort_by_key(|c| c.title.to_lowercase());
    Ok(channels)
}

pub fn load(config: &Path, id: &str) -> Result<Channel> {
    if !id_ok(id) {
        return Err(invalid("Choose a valid lowercase channel ID."));
    }
    let root = config.join("channels").join(id);
    if fs::symlink_metadata(&root)
        .map_err(|e| CoreError::io(&root, e))?
        .file_type()
        .is_symlink()
    {
        return Err(invalid("The channel folder cannot be a symbolic link."));
    }
    let text = read_limited(&document_path(&root, "channel.md")?, MAX_DOCUMENT_BYTES)?;
    Ok(Channel {
        id: id.into(),
        title: title(&text, id),
        root,
    })
}

pub fn create(config: &Path, name: &str) -> Result<Channel> {
    let name = name.trim();
    let id = slug(name);
    if name.is_empty()
        || name.chars().count() > 120
        || name.chars().any(char::is_control)
        || !id_ok(&id)
    {
        return Err(invalid(
            "Give the channel a name up to 120 characters with at least one letter or number.",
        ));
    }
    if list(config)?.len() >= MAX_CHANNELS {
        return Err(invalid("The channel library is limited to 200 channels."));
    }
    let parent = config.join("channels");
    fs::create_dir_all(&parent).map_err(|e| CoreError::io(&parent, e))?;
    let root = parent.join(&id);
    fs::create_dir(&root).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            invalid(format!("Channel {id} already exists. Choose another name."))
        } else {
            CoreError::io(&root, e)
        }
    })?;
    let channel = Channel {
        id,
        title: name.into(),
        root,
    };
    let mut created: Vec<PathBuf> = Vec::new();
    for (file, text) in templates(name) {
        if let Err(e) = channel.add_document(file, &text) {
            let mut leftovers = Vec::new();
            for path in created {
                if let Err(cleanup) = fs::remove_file(&path) {
                    leftovers.push(format!("{}: {cleanup}", path.display()));
                }
            }
            if let Err(cleanup) = fs::remove_dir(&channel.root) {
                leftovers.push(format!("{}: {cleanup}", channel.root.display()));
            }
            return Err(invalid(format!(
                "Could not initialize the channel: {e}.{}",
                if leftovers.is_empty() {
                    String::new()
                } else {
                    format!(" Incomplete files remain: {}", leftovers.join("; "))
                }
            )));
        }
        created.push(channel.root.join(file));
    }
    Ok(channel)
}

fn templates(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("channel.md", format!("# {name}\n\n## Purpose and audience\nDescribe who watches, what they should learn or feel, and the channel promise.\n\n## Relationship to Vision\nChannel is the durable identity and knowledge. A video's Vision is its creative brief and catalog/pacing preferences. Record a preferred Vision here if useful.\n\n## Non-negotiables\nAdd channel-specific requirements.\n")),
        ("style.md", "# Style guide\n\n## Writing and storytelling\nAdd tone, structure, hooks, terminology and examples.\n\n## Visual language\nAdd composition, colors, typography, recurring graphics and reference links.\n\n## Pacing and sound\nAdd editing rhythm, silence, music and SFX preferences.\n".into()),
        ("lessons.md", "# Lessons learned\n\nRecord dated observations after reviewing videos. Include the episode, evidence, what worked or failed, and the specific change to try next. Keep experiments separate from established rules.\n".into()),
        ("staples.md", "# Staple assets\n\nList recurring music, sound effects, graphics, intros, outros and visual motifs. For each, record the actual asset path or catalog ID, role, when to use it, license/source, and relevant settings. Reuse approved assets before generating replacements.\n".into()),
        ("tools.md", "# Production tools\n\n## ComfyUI\nRecord the local endpoint, API-format workflow paths, required models/nodes, input mapping, prompts, seeds, output folders and verification steps. Discover live capabilities before submitting a job.\n\n## VoiceStudio\nRecord the installed app's endpoint or MCP connection, engine, saved voice/profile ID, pronunciation guidance, delivery and export settings. Use the configured voice for voiceovers.\n\n## FilmCraft\nRecord the target project/sequence and available MCP or control connection. Discover live commands; do not assume Spear Fia's Sync placeholder already transfers a timeline.\n".into()),
        ("production.md", WORKFLOW.into()),
    ]
}

pub const WORKFLOW: &str = "# Channel production workflow\n\n1. Read every channel Markdown guide, the video's channel.md snapshot, its Vision, and current script. Imported analyses are references, not evidence of user approval.\n2. Draft the script for this channel and request the creator's approval. Before approval, prepare plans only: do not generate media or voiceovers, assemble the final edit, or push to FilmCraft.\n3. After the creator approves the saved script and channel brief in Spear Fia, obtain the production handoff. Recheck approval before starting expensive jobs; script or brief changes require another approval.\n4. Plan each section/line's visuals and audio. Reuse channel staples. Through the available ComfyUI connection, generate only needed visuals, graphics and music, using the channel's documented workflows/settings. Use the installed VoiceStudio app's discovered API/MCP connection and specified voice for narration. Preserve prompts, seeds, voice settings and output paths in production notes.\n5. Verify actual outputs (content, dimensions, duration, narration/pronunciation and audio quality). Store them in the project's media folders or linked external folders. Keep missing/failed assets explicit and retry only the failed work. Never claim a queued job is a finished file.\n6. Link generated and staple media to the correct words, lines or sections in Spear Fia. Choose transitions, effects, music and sound effects only where needed; a plain cut or no extra sound is valid. Preserve existing script/media anchors.\n7. Use a verified FilmCraft MCP/control integration to assemble the review timeline. If that integration is unavailable, report the missing capability and retain the ready assets/plan; do not claim a successful push.\n8. Report the FilmCraft project/sequence and outstanding issues for creator review. Stop before publishing. After review, add dated, evidence-backed lessons to the shared channel guides for future videos.\n\nCurrent scope: channel knowledge, snapshots, script approval and agent handoff are implemented. Automatic ComfyUI/VoiceStudio generation and Spear Fia-to-FilmCraft sync are not implemented by this handoff. Discover and use external tool connections when available.\n";

impl Channel {
    pub fn documents(&self) -> Result<Vec<Document>> {
        let mut docs = Vec::new();
        let mut bytes = 0usize;
        for entry in fs::read_dir(&self.root).map_err(|e| CoreError::io(&self.root, e))? {
            let entry = entry.map_err(|e| CoreError::io(&self.root, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".md") {
                continue;
            }
            if docs.len() == MAX_DOCUMENTS {
                return Err(invalid("Use at most 32 Markdown documents per channel."));
            }
            let path = document_path(&self.root, &name)?;
            let text = read_limited(&path, MAX_DOCUMENT_BYTES)?;
            bytes = bytes.saturating_add(text.len());
            if bytes > MAX_BRIEF_BYTES / 2 {
                return Err(invalid(
                    "The channel's documents exceed the 1 MiB combined limit.",
                ));
            }
            docs.push(Document { name, text });
        }
        docs.sort_by_key(|d| (d.name != "channel.md", d.name.clone()));
        Ok(docs)
    }

    pub fn save_document(&self, name: &str, expected: &str, text: &str) -> Result<()> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("A channel document must be under 256 KiB."));
        }
        let path = document_path(&self.root, name)?;
        if read_limited(&path, MAX_DOCUMENT_BYTES)? != expected {
            return Err(invalid(
                "This document changed outside the editor. Copy your draft, then reload before saving.",
            ));
        }
        let bytes = self
            .documents()?
            .iter()
            .map(|d| d.text.len())
            .sum::<usize>();
        if bytes
            .saturating_sub(expected.len())
            .saturating_add(text.len())
            > MAX_BRIEF_BYTES / 2
        {
            return Err(invalid(
                "The channel's documents exceed the 1 MiB combined limit.",
            ));
        }
        write_text(&path, text)
    }

    pub fn add_document(&self, name: &str, text: &str) -> Result<()> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("A channel document must be under 256 KiB."));
        }
        let docs = self.documents()?;
        if docs.len() >= MAX_DOCUMENTS
            || docs
                .iter()
                .map(|d| d.text.len())
                .sum::<usize>()
                .saturating_add(text.len())
                > MAX_BRIEF_BYTES / 2
        {
            return Err(invalid(
                "The channel has reached its document or combined size limit.",
            ));
        }
        let path = document_path(&self.root, name)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| CoreError::io(&path, e))?;
        if let Err(e) = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            return match fs::remove_file(&path) {
                Ok(()) => Err(CoreError::io(&path, e)),
                Err(cleanup) => Err(invalid(format!(
                    "{} could not be saved: {e}. Incomplete document could not be removed: {cleanup}",
                    path.display()
                ))),
            };
        }
        Ok(())
    }

    pub fn brief(&self) -> Result<String> {
        let mut brief = format!(
            "# Channel: {}\n\nShared channel folder: {}\n\nThis video uses a snapshot of the guides below. Refresh the project brief explicitly to adopt later channel changes; review and approve again after changes.\n",
            self.title,
            self.root.display()
        );
        for doc in self.documents()? {
            brief.push_str(&format!("\n---\n\n## {}\n\n{}\n", doc.name, doc.text));
        }
        validate_brief(&brief)?;
        Ok(brief)
    }
}

pub fn validate_brief(text: &str) -> Result<()> {
    if text.trim().is_empty() || text.len() > MAX_BRIEF_BYTES {
        return Err(invalid(
            "The channel brief must contain text and fit within 2 MiB.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    NoChannel,
    Draft,
    Unsaved,
    ScriptChanged,
    BriefChanged,
    Approved,
}

impl Approval {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoChannel => "Attach a channel brief first.",
            Self::Draft => "Script needs your approval.",
            Self::Unsaved => "Save the script before approving or handing it off.",
            Self::ScriptChanged => "Script changed since approval. Review and approve again.",
            Self::BriefChanged => "Channel brief changed since approval. Review and approve again.",
            Self::Approved => "Approved for production with this script and channel brief.",
        }
    }
}

fn optional(path: &Path, limit: usize) -> Result<Option<String>> {
    match read_limited(path, limit) {
        Ok(text) => Ok(Some(text)),
        Err(CoreError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

impl Project {
    pub fn channel_brief(&self) -> Result<Option<String>> {
        optional(&self.root.join(PROJECT_BRIEF), MAX_BRIEF_BYTES)
    }

    pub fn set_channel_brief(&self, expected: Option<&str>, text: &str) -> Result<()> {
        validate_brief(text)?;
        if self.channel_brief()?.as_deref() != expected {
            return Err(invalid(
                "The project's channel brief changed. Reload before replacing it.",
            ));
        }
        write_text(&self.root.join(PROJECT_BRIEF), text)
    }

    pub fn script_approval(&self, buffer: Option<&str>) -> Result<Approval> {
        let Some(brief) = self.channel_brief()? else {
            return Ok(Approval::NoChannel);
        };
        let script = read_limited(&self.script_path(), MAX_BRIEF_BYTES)?;
        if buffer.is_some_and(|b| b != script) {
            return Ok(Approval::Unsaved);
        }
        let Some(approved_script) = optional(&self.root.join(APPROVED_SCRIPT), MAX_BRIEF_BYTES)?
        else {
            return Ok(Approval::Draft);
        };
        if script != approved_script {
            return Ok(Approval::ScriptChanged);
        }
        if optional(&self.root.join(APPROVED_CHANNEL), MAX_BRIEF_BYTES)?.as_deref() != Some(&brief)
        {
            return Ok(Approval::BriefChanged);
        }
        Ok(Approval::Approved)
    }

    /// Called by the creator's explicit approval button, never by an autonomous agent.
    pub fn approve_script(&self, buffer: &str) -> Result<()> {
        let brief = self
            .channel_brief()?
            .ok_or_else(|| invalid("Attach a channel before approving the script."))?;
        validate_brief(&brief)?;
        let script = read_limited(&self.script_path(), MAX_BRIEF_BYTES)?;
        if script.trim().is_empty() || script != buffer {
            return Err(invalid(
                "Save a nonempty script before approving it. The saved file must match the editor.",
            ));
        }
        write_text(&self.root.join(APPROVED_SCRIPT), &script)?;
        write_text(&self.root.join(APPROVED_CHANNEL), &brief)
    }

    pub fn revoke_script_approval(&self) -> Result<()> {
        let path = self.root.join(APPROVED_SCRIPT);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    pub fn agent_brief(&self, production: bool, buffer: Option<&str>) -> Result<String> {
        let status = self.script_approval(buffer)?;
        if production && status != Approval::Approved {
            return Err(invalid(status.label()));
        }
        let brief = self
            .channel_brief()?
            .ok_or_else(|| invalid("Attach a channel brief first."))?;
        let script = read_limited(&self.script_path(), MAX_BRIEF_BYTES)?;
        let vision = read_limited(&self.vision_path(), MAX_DOCUMENT_BYTES)?;
        // Bind the exported bytes too: files may change after the initial status check.
        if production
            && (optional(&self.root.join(APPROVED_SCRIPT), MAX_BRIEF_BYTES)?.as_deref()
                != Some(script.as_str())
                || optional(&self.root.join(APPROVED_CHANNEL), MAX_BRIEF_BYTES)?.as_deref()
                    != Some(brief.as_str())
                || buffer.is_some_and(|b| b != script))
        {
            return Err(invalid(
                "The script or channel brief changed while preparing the handoff. Review approval and retry.",
            ));
        }
        let mut output = format!(
            "# {} handoff: {}\n\nProject folder: {}\n\nApproval: {}\n\n{}\n\n{}\n\n## Project Vision ({})\n\n{}\n\n## Saved script ({})\n\n{}\n",
            if production { "Production" } else { "Drafting" },
            self.name(),
            self.root.display(),
            status.label(),
            if production {
                "Use this approved script. Recheck approval before generation or editor assembly. This document does not grant permission to publish."
            } else {
                "Draft and plan only. Obtain the creator's script approval in Spear Fia before generating media, voiceovers or assembling a timeline."
            },
            brief,
            self.file.vision,
            vision,
            self.file.script,
            script
        );
        output.push_str(&format!("\n---\n\n{WORKFLOW}\n## Recheck approval\n\nRun `spearfia project handoff \"{}\"` before production. The command fails if approval is absent or the saved script/brief changed. Agents must not write approval snapshots themselves.\n", self.root.display()));
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_dir;

    fn project() -> Project {
        let root = temp_dir("channel-project");
        fs::create_dir(root.join("script")).unwrap();
        let p = Project {
            root,
            file: serde_json::from_value(
                serde_json::json!({"name":"Episode", "spec_version":1, "adapter":"filmcraft"}),
            )
            .unwrap(),
        };
        p.write_script("## Hook\nThe approved words.\n").unwrap();
        fs::write(p.vision_path(), "extends: cinematic\n").unwrap();
        p.save().unwrap();
        p
    }

    #[test]
    fn creates_plain_markdown_library_imports_and_reopens() {
        let dir = temp_dir("channels");
        let c = create(&dir, "Science Channel").unwrap();
        assert_eq!(c.id, "science-channel");
        assert_eq!(c.documents().unwrap().len(), 6);
        c.add_document(
            "analysis.md",
            "# Channel analysis\nUse original explanatory diagrams.",
        )
        .unwrap();
        let reopened = load(&dir, "science-channel").unwrap();
        let brief = reopened.brief().unwrap();
        for value in [
            "style.md",
            "lessons.md",
            "staples.md",
            "analysis.md",
            "VoiceStudio",
            "ComfyUI",
            "FilmCraft",
            "original explanatory diagrams",
        ] {
            assert!(brief.contains(value), "{value}");
        }
        assert_eq!(list(&dir).unwrap(), [reopened]);
        assert!(create(&dir, "Science Channel").is_err());
        assert!(create(&dir, "SCIENCE CHANNEL").is_err());
    }

    #[test]
    fn rejects_paths_duplicates_oversized_and_stale_edits() {
        let dir = temp_dir("channel-inputs");
        let c = create(&dir, "A channel").unwrap();
        let doc = c
            .documents()
            .unwrap()
            .into_iter()
            .find(|d| d.name == "style.md")
            .unwrap();
        c.save_document(&doc.name, &doc.text, "# Style\nNew rule")
            .unwrap();
        assert!(
            c.save_document(&doc.name, &doc.text, "stale overwrite")
                .is_err()
        );
        assert_eq!(
            read_limited(&c.root.join("style.md"), MAX_DOCUMENT_BYTES).unwrap(),
            "# Style\nNew rule"
        );
        assert!(c.add_document("style.md", "overwrite").is_err());
        c.add_document("AGENTS.md", "# Channel notes").unwrap();
        assert!(c.brief().unwrap().contains("AGENTS.md"));
        for name in [
            "../outside.md",
            "con.md",
            "folder/file.md",
            "..\\escape.md",
            "bad.txt",
        ] {
            assert!(c.add_document(name, "text").is_err(), "{name}");
        }
        for id in ["../outside", "", "CON", "-name", "name-", "a/b"] {
            assert!(load(&dir, id).is_err(), "{id}");
        }
        assert!(
            c.add_document("large.md", &"a".repeat(MAX_DOCUMENT_BYTES + 1))
                .is_err()
        );
        fs::write(c.root.join("large.md"), vec![b'a'; MAX_DOCUMENT_BYTES + 1]).unwrap();
        assert!(c.brief().is_err());
    }

    #[test]
    fn per_video_snapshot_is_stable_until_refreshed_and_never_clobbered() {
        let dir = temp_dir("channel-snapshot");
        let c = create(&dir, "One").unwrap();
        let p = project();
        let original = c.brief().unwrap();
        p.set_channel_brief(None, &original).unwrap();
        let doc = c
            .documents()
            .unwrap()
            .into_iter()
            .find(|d| d.name == "lessons.md")
            .unwrap();
        c.save_document(
            &doc.name,
            &doc.text,
            "# Lessons\nEpisode 1: simplify the opening diagram.",
        )
        .unwrap();
        assert_eq!(p.channel_brief().unwrap(), Some(original.clone()));
        assert!(p.set_channel_brief(None, &c.brief().unwrap()).is_err());
        let next = c.brief().unwrap();
        p.set_channel_brief(Some(&original), &next).unwrap();
        assert_eq!(p.channel_brief().unwrap(), Some(next));
        assert!(p.set_channel_brief(Some(&original), "stale").is_err());
    }

    #[test]
    fn production_requires_explicit_approval_of_saved_script_and_exact_brief() {
        let p = project();
        assert_eq!(p.script_approval(None).unwrap(), Approval::NoChannel);
        assert!(p.approve_script(&p.read_script().unwrap()).is_err());
        p.set_channel_brief(None, "# Channel\nUse diagrams.")
            .unwrap();
        let script = p.read_script().unwrap();
        assert_eq!(p.script_approval(None).unwrap(), Approval::Draft);
        assert!(p.agent_brief(true, None).is_err());
        assert!(
            p.agent_brief(false, None)
                .unwrap()
                .contains("Draft and plan only")
        );
        assert!(p.approve_script("unsaved edits").is_err());
        p.approve_script(&script).unwrap();
        assert_eq!(
            p.script_approval(Some(&script)).unwrap(),
            Approval::Approved
        );
        let brief = p.agent_brief(true, None).unwrap();
        assert!(
            brief.contains(&script)
                && brief.contains("VoiceStudio")
                && brief.contains("FilmCraft")
                && brief.contains("extends: cinematic")
        );
        assert_eq!(
            p.script_approval(Some("unsaved edit")).unwrap(),
            Approval::Unsaved
        );
        assert!(p.agent_brief(true, Some("unsaved edit")).is_err());
        p.write_script("New script").unwrap();
        assert_eq!(p.script_approval(None).unwrap(), Approval::ScriptChanged);
        assert!(p.agent_brief(true, None).is_err());
        p.approve_script("New script").unwrap();
        p.set_channel_brief(Some("# Channel\nUse diagrams."), "# Channel\nUse closeups.")
            .unwrap();
        assert_eq!(p.script_approval(None).unwrap(), Approval::BriefChanged);
        assert!(p.agent_brief(true, None).is_err());
        p.approve_script("New script").unwrap();
        p.revoke_script_approval().unwrap();
        assert_eq!(p.script_approval(None).unwrap(), Approval::Draft);
        assert!(p.agent_brief(true, None).is_err());
    }

    #[test]
    fn failed_approval_write_never_marks_a_changed_brief_as_approved() {
        let p = project();
        p.set_channel_brief(None, "# Channel\nOld brief").unwrap();
        let script = p.read_script().unwrap();
        p.approve_script(&script).unwrap();
        p.set_channel_brief(Some("# Channel\nOld brief"), "# Channel\nNew brief")
            .unwrap();
        fs::create_dir(p.root.join(format!(
            "sync/.approved-channel.md.{}.tmp",
            std::process::id()
        )))
        .unwrap();
        assert!(p.approve_script(&script).is_err());
        assert_eq!(p.script_approval(None).unwrap(), Approval::BriefChanged);
        assert!(p.agent_brief(true, None).is_err());
    }

    #[test]
    fn failed_document_save_preserves_the_original() {
        let c = create(&temp_dir("channel-save-failure"), "One").unwrap();
        let doc = c
            .documents()
            .unwrap()
            .into_iter()
            .find(|d| d.name == "style.md")
            .unwrap();
        fs::create_dir(c.root.join(format!(".style.md.{}.tmp", std::process::id()))).unwrap();
        assert!(
            c.save_document(&doc.name, &doc.text, "replacement")
                .is_err()
        );
        assert_eq!(
            read_limited(&c.root.join("style.md"), MAX_DOCUMENT_BYTES).unwrap(),
            doc.text
        );
    }
}
