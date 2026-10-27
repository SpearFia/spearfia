//! Project model: `project.json` and the project folder around it.
//!
//! The fields under "Spear Fia extensions" are allowed by the schema's
//! `additionalProperties` but are not named in `spearfia-spec` yet. They need adding there.

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.json";
pub const SCHEMA_URL: &str = "https://spearfia.dev/schemas/project/v1.json";
pub const SPEC_VERSION: u32 = 1;

/// Rational frame rate, e.g. 60000/1001 for 59.94 fps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timebase {
    pub num: u32,
    pub den: u32,
}

impl Timebase {
    pub const FPS_23_976: Self = Self::new(24000, 1001);
    pub const FPS_24: Self = Self::new(24, 1);
    pub const FPS_25: Self = Self::new(25, 1);
    pub const FPS_29_97: Self = Self::new(30000, 1001);
    pub const FPS_30: Self = Self::new(30, 1);
    pub const FPS_50: Self = Self::new(50, 1);
    pub const FPS_59_94: Self = Self::new(60000, 1001);
    pub const FPS_60: Self = Self::new(60, 1);

    /// The rates offered in the UI, most common first for YouTube work.
    pub const COMMON: [Self; 8] = [
        Self::FPS_59_94,
        Self::FPS_60,
        Self::FPS_29_97,
        Self::FPS_30,
        Self::FPS_23_976,
        Self::FPS_24,
        Self::FPS_25,
        Self::FPS_50,
    ];

    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    pub fn fps(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// "59.94", "60", "23.976".
    pub fn label(self) -> String {
        if self.den == 1 {
            return self.num.to_string();
        }
        let s = format!("{:.3}", self.fps());
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// What kind of video the project is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Short,
    #[default]
    LongForm,
    Square,
}

impl Format {
    pub const ALL: [Self; 3] = [Self::Short, Self::LongForm, Self::Square];

    pub fn label(self) -> &'static str {
        match self {
            Self::Short => "Short 9:16",
            Self::LongForm => "Long-form 16:9",
            Self::Square => "Square 1:1",
        }
    }

    /// Width × height in pixels.
    pub fn default_resolution(self) -> [u32; 2] {
        match self {
            Self::Short => [1080, 1920],
            Self::LongForm => [3840, 2160],
            Self::Square => [1080, 1080],
        }
    }

    /// The value written into the script front matter.
    pub fn script_value(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::LongForm => "long",
            Self::Square => "square",
        }
    }
}

/// What a media folder holds. Subfolders inherit their parent's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaRole {
    ARoll,
    Voiceover,
    BRoll,
    Screen,
    Graphics,
    Other,
}

impl MediaRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::ARoll => "A-roll",
            Self::Voiceover => "VO",
            Self::BRoll => "B-roll",
            Self::Screen => "Screen",
            Self::Graphics => "Graphics",
            Self::Other => "Other",
        }
    }
}

/// A folder inside (or linked to) the project, with its role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFolder {
    /// Project-relative or absolute path, `/`-separated.
    pub path: String,
    pub role: MediaRole,
}

impl MediaFolder {
    pub fn new(path: impl Into<String>, role: MediaRole) -> Self {
        Self {
            path: path.into(),
            role,
        }
    }

    /// The folders a new project gets by default.
    pub fn defaults() -> Vec<Self> {
        vec![
            Self::new("A-roll/facecam", MediaRole::ARoll),
            Self::new("A-roll/voiceover", MediaRole::Voiceover),
            Self::new("B-roll", MediaRole::BRoll),
            Self::new("Screen", MediaRole::Screen),
            Self::new("Graphics", MediaRole::Graphics),
        ]
    }
}

/// How Spear Fia talks to FilmCraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum FilmCraftConnection {
    /// The running app, started with `filmcraft --control <port>`.
    Bridge { port: u16 },
    /// `filmcraft-cli mcp --project <file>`, no window.
    Headless,
}

impl Default for FilmCraftConnection {
    fn default() -> Self {
        Self::Bridge { port: 9876 }
    }
}

/// The FilmCraft project paired with this Spear Fia project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilmCraftLink {
    /// Path to the `.fcproj`, project-relative or absolute.
    pub project_file: String,
    pub sequence: String,
    #[serde(default)]
    pub connection: FilmCraftConnection,
    /// False until the `.fcproj` actually exists (it is made on the first push).
    #[serde(default)]
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub timebase: Timebase,
    pub resolution: [u32; 2],
    pub sample_rate: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timebase: Timebase::FPS_59_94,
            resolution: Format::LongForm.default_resolution(),
            sample_rate: 48_000,
        }
    }
}

fn default_script() -> String {
    "script/script.md".into()
}

fn default_vision() -> String {
    "vision.yaml".into()
}

fn default_schema() -> String {
    SCHEMA_URL.into()
}

/// The contents of `project.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    #[serde(rename = "$schema", default = "default_schema")]
    pub schema: String,
    pub name: String,
    pub spec_version: u32,
    pub adapter: String,
    #[serde(default)]
    pub library_roots: Vec<String>,
    #[serde(default = "default_script")]
    pub script: String,
    #[serde(default = "default_vision")]
    pub vision: String,
    #[serde(default)]
    pub settings: Settings,

    // ---- Spear Fia extensions (not yet named in spearfia-spec) ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default)]
    pub format: Format,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_runtime_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_folders: Vec<MediaFolder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filmcraft: Option<FilmCraftLink>,
}

/// An open project: its folder and its `project.json`.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub file: ProjectFile,
}

impl Project {
    /// Open the project in `root` (the folder that holds `project.json`).
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let path = root.join(PROJECT_FILE);
        let text = fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))?;
        let file = serde_json::from_str(&text).map_err(|e| CoreError::json(&path, e))?;
        Ok(Self { root, file })
    }

    pub fn save(&self) -> Result<()> {
        write_json(&self.root.join(PROJECT_FILE), &self.file)
    }

    pub fn name(&self) -> &str {
        &self.file.name
    }

    /// Resolve a project-relative path (absolute paths pass through).
    pub fn resolve(&self, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    pub fn script_path(&self) -> PathBuf {
        self.resolve(&self.file.script)
    }

    pub fn vision_path(&self) -> PathBuf {
        self.resolve(&self.file.vision)
    }

    pub fn read_script(&self) -> Result<String> {
        let path = self.script_path();
        fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))
    }

    pub fn write_script(&self, text: &str) -> Result<()> {
        let path = self.script_path();
        write_text(&path, text)
    }

    /// The template named in a JSON or simple YAML project vision.
    pub fn vision_preset(&self) -> Option<String> {
        let text = fs::read_to_string(self.vision_path()).ok()?;
        if let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) {
            return document
                .get("extends")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
        }
        text.lines()
            .find_map(|l| l.strip_prefix("extends:"))
            .map(|v| v.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    }
}

/// How the script starts out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptStart {
    /// Title, then Hook / Main / Outro sections.
    Sections,
    /// Copy an existing Markdown or text file.
    Import(PathBuf),
}

/// Everything the New Project screen collects.
#[derive(Debug, Clone)]
pub struct NewProject {
    pub name: String,
    /// Snapshot of shared channel guidance, stored as ordinary channel.md.
    pub channel_brief: Option<String>,
    pub series: Option<String>,
    /// The project folder. Created if missing; may already hold media.
    pub root: PathBuf,
    pub format: Format,
    pub resolution: [u32; 2],
    pub timebase: Timebase,
    pub target_runtime_seconds: Option<u32>,
    pub vision: NewVision,
    pub script: ScriptStart,
    /// Subfolders to create (if missing) and record with their roles.
    pub media_folders: Vec<MediaFolder>,
    pub filmcraft: Option<FilmCraftLink>,
}

/// The project vision written to `vision.yaml`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewVision {
    /// Complete template embedded in the project so custom visions are portable.
    pub template: Option<serde_json::Value>,
    /// Preset name, e.g. `chaotic-gaming-comedy`.
    pub extends: Option<String>,
    pub goal: String,
    pub avg_shot_seconds: Option<f32>,
    /// `jump`, `invisible`, `rhythmic` or `mixed`.
    pub cut_style: Option<String>,
}

/// Create a project on disk and return it opened.
///
/// Writes `project.json`, `vision.yaml`, `script/script.md`, `links.json` and
/// `.gitignore`, and creates the media, `analysis/`, `sync/`, `cache/` and `exports/`
/// folders. Refuses to touch a folder that already holds a project.
pub fn create_project(spec: &NewProject) -> Result<Project> {
    let name = spec.name.trim();
    let vision_source = vision_yaml(name, &spec.vision)?;
    if name.is_empty() {
        return Err(CoreError::Invalid("The project needs a name.".into()));
    }
    if spec.root.as_os_str().is_empty() {
        return Err(CoreError::Invalid("Choose a project folder.".into()));
    }
    let root = spec.root.clone();
    if root.join(PROJECT_FILE).exists() {
        return Err(CoreError::Invalid(format!(
            "{} already holds a Spear Fia project.",
            root.display()
        )));
    }
    if let Some(brief) = &spec.channel_brief {
        crate::channels::validate_brief(brief)?;
        if root.join(crate::channels::PROJECT_BRIEF).exists() {
            return Err(CoreError::Invalid(
                "The project folder already contains channel.md; it was not overwritten.".into(),
            ));
        }
    }
    let script_text = match &spec.script {
        ScriptStart::Sections => starter_script(name, spec.format),
        ScriptStart::Import(path) => imported_script(path, name, spec.format)?,
    };

    for dir in ["script", "analysis", "sync", "cache", "exports"] {
        create_dir(&root.join(dir))?;
    }
    for folder in &spec.media_folders {
        create_dir(&root.join(&folder.path))?;
    }

    let file = ProjectFile {
        schema: SCHEMA_URL.into(),
        name: name.to_owned(),
        spec_version: SPEC_VERSION,
        adapter: "filmcraft".into(),
        library_roots: vec![],
        script: default_script(),
        vision: default_vision(),
        settings: Settings {
            timebase: spec.timebase,
            resolution: spec.resolution,
            sample_rate: 48_000,
        },
        series: spec
            .series
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Into::into),
        format: spec.format,
        target_runtime_seconds: spec.target_runtime_seconds,
        media_folders: spec.media_folders.clone(),
        filmcraft: spec.filmcraft.clone(),
    };
    let project = Project { root, file };

    write_text(&project.vision_path(), &vision_source)?;
    project.write_script(&script_text)?;
    if let Some(brief) = &spec.channel_brief {
        project.set_channel_brief(None, brief)?;
    }
    write_text(&project.root.join("links.json"), "{\n  \"links\": []\n}\n")?;
    write_text(&project.root.join(".gitignore"), "cache/\nexports/\n")?;
    project.save()?;
    Ok(project)
}

/// The script from an imported `.md`, `.txt` or `.docx`, with front matter added when it
/// has none. What a `.docx` couldn't carry over is noted at the top.
fn imported_script(path: &Path, title: &str, format: Format) -> Result<String> {
    let is_docx = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("docx"));
    let body = if is_docx {
        let c = crate::docx::import(path)?;
        let file = path
            .file_name()
            .map_or_else(Default::default, |n| n.to_string_lossy());
        if c.skipped.is_empty() {
            c.markdown
        } else {
            format!(
                "> Imported from {file}. Left out: {}.\n\n{}",
                c.skipped.join(", "),
                c.markdown
            )
        }
    } else {
        fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?
    };
    if body.starts_with("---\n") || body.starts_with("---\r\n") {
        return Ok(body);
    }
    Ok(format!(
        "---\ntitle: {}\nformat: {}\nvision: ../vision.yaml\n---\n\n{body}",
        yaml_str(title),
        format.script_value()
    ))
}

fn starter_script(title: &str, format: Format) -> String {
    format!(
        "---\ntitle: {title}\nformat: {}\nvision: ../vision.yaml\n---\n\n## Hook\n\n## Main\n\n## Outro\n",
        format.script_value()
    )
}

fn vision_yaml(name: &str, v: &NewVision) -> Result<String> {
    if let Some(document) = &v.template {
        let source =
            serde_json::to_string(document).map_err(|e| CoreError::Invalid(e.to_string()))?;
        let template = crate::visions::Vision::parse(&source)?;
        let mut document = template.document;
        document["name"] = slug(name).into();
        if let Some(extends) = &v.extends {
            document["extends"] = extends.clone().into();
        }
        if !v.goal.trim().is_empty() {
            document["goal"] = v.goal.trim().into();
        }
        if v.avg_shot_seconds.is_some() || v.cut_style.is_some() {
            if document.get("pacing").is_none() {
                document["pacing"] = serde_json::json!({});
            }
            if let Some(seconds) = v.avg_shot_seconds {
                if !seconds.is_finite() || seconds <= 0.0 {
                    return Err(CoreError::Invalid(
                        "Average shot length must be positive and finite.".into(),
                    ));
                }
                document["pacing"]["avg_shot_seconds"] = seconds.into();
            }
            if let Some(style) = &v.cut_style {
                if !crate::visions::CUT_STYLES.contains(&style.as_str()) {
                    return Err(CoreError::Invalid("Unknown cut style.".into()));
                }
                document["pacing"]["cut_style"] = style.clone().into();
            }
        }
        return serde_json::to_string_pretty(&document)
            .map_err(|e| CoreError::Invalid(e.to_string()));
    }
    let mut out = String::from(
        "# yaml-language-server: $schema=https://spearfia.dev/schemas/vision/v1.json\n",
    );
    out += &format!("name: {}\n", yaml_str(&slug(name)));
    if let Some(preset) = &v.extends {
        out += &format!("extends: {}\n", yaml_str(preset));
    }
    if !v.goal.trim().is_empty() {
        out += &format!("goal: {}\n", yaml_str(v.goal.trim()));
    }
    if v.avg_shot_seconds.is_some() || v.cut_style.is_some() {
        out += "pacing:\n";
        if let Some(s) = v.avg_shot_seconds {
            out += &format!("  avg_shot_seconds: {s}\n");
        }
        if let Some(c) = &v.cut_style {
            out += &format!("  cut_style: {}\n", yaml_str(c));
        }
    }
    Ok(out)
}

/// Quote a YAML scalar only when needed.
fn yaml_str(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        && !s.starts_with(' ')
        && !s.ends_with(' ');
    if plain {
        s.to_owned()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// `Ep 2 · First Contact` → `ep-2-first-contact`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| CoreError::io(path, e))
}

/// Write `text` to `path` atomically: a crash mid-write leaves the old file, never a
/// truncated one. Writes a temp file beside it, flushes it to disk, then renames over it.
pub(crate) fn write_text(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy());
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let written = fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(CoreError::io(path, e));
    }
    Ok(())
}

/// Refuse to overwrite a JSON file that exists but can't be read as `T`: saving over it
/// would replace whatever is in it with what the app holds (often an empty list).
pub(crate) fn ensure_loadable<T: serde::de::DeserializeOwned>(path: &Path) -> Result<()> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<T>(&text).map(|_| ()).map_err(|e| {
            CoreError::Invalid(format!(
                "{} can't be read ({e}), so it wasn't overwritten. Fix or delete it, then try again.",
                path.display()
            ))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(CoreError::io(path, e)),
    }
}

/// Write `value` as pretty JSON, atomically (temp file, then rename).
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value).map_err(|e| CoreError::json(path, e))?;
    text.push('\n');
    write_text(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_dir;

    #[test]
    fn writes_atomically_and_leaves_no_temp_file() {
        let dir = temp_dir("atomic");
        let path = dir.join("sub/script.md");
        write_text(&path, "first").unwrap();
        write_text(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["script.md"]);
    }

    #[test]
    fn imports_get_front_matter_once() {
        let dir = temp_dir("import");
        let txt = dir.join("draft.txt");
        fs::write(&txt, "## Hook\nNever drifted.\n").unwrap();
        let out = imported_script(&txt, "Ep: 2", Format::Short).unwrap();
        assert!(
            out.starts_with("---\ntitle: \"Ep: 2\"\nformat: short\n"),
            "{out}"
        );
        assert!(out.ends_with("## Hook\nNever drifted.\n"));
        let md = dir.join("s.md");
        fs::write(&md, "---\ntitle: Mine\n---\nx\n").unwrap();
        assert_eq!(
            imported_script(&md, "Other", Format::Short).unwrap(),
            "---\ntitle: Mine\n---\nx\n"
        );
    }

    #[test]
    fn refuses_to_overwrite_unreadable_json() {
        let dir = temp_dir("loadable");
        let path = dir.join("links.json");
        assert!(
            ensure_loadable::<serde_json::Value>(&path).is_ok(),
            "missing is fine"
        );
        fs::write(&path, "{ not json").unwrap();
        let err = ensure_loadable::<serde_json::Value>(&path).unwrap_err();
        assert!(err.to_string().contains("wasn't overwritten"), "{err}");
    }

    fn spec(root: PathBuf) -> NewProject {
        NewProject {
            channel_brief: None,
            name: "Ep 2 · First Contact".into(),
            series: Some("Learning to Drift".into()),
            root,
            format: Format::LongForm,
            resolution: Format::LongForm.default_resolution(),
            timebase: Timebase::FPS_59_94,
            target_runtime_seconds: Some(720),
            vision: NewVision {
                template: None,
                extends: Some("chaotic-gaming-comedy".into()),
                goal: "Make the crashes funny, then earn the slide.".into(),
                avg_shot_seconds: Some(1.5),
                cut_style: Some("jump".into()),
            },
            script: ScriptStart::Sections,
            media_folders: MediaFolder::defaults(),
            filmcraft: Some(FilmCraftLink {
                project_file: "Ep2.fcproj".into(),
                sequence: "Part 1".into(),
                connection: FilmCraftConnection::default(),
                created: false,
            }),
        }
    }

    #[test]
    fn custom_vision_is_embedded_without_losing_guidance() {
        let root = temp_dir("custom-vision").join("episode");
        let mut request = spec(root);
        request.vision.extends = Some("science-explainer".into());
        request.vision.goal.clear();
        request.vision.template = Some(serde_json::json!({
            "name":"science-explainer", "goal":"Explain clearly",
            "pacing":{"max_silence_seconds":0.5},
            "script_rules":["Explain one concept at a time"],
            "weights":{"diagram":0.9}, "avoid":["Jargon"],
            "references":[{"url":"https://example.com/analysis","note":"Source"}],
            "notes":"Keep the source assumptions"
        }));
        let project = create_project(&request).unwrap();
        assert_eq!(
            project.vision_preset().as_deref(),
            Some("science-explainer")
        );
        let document: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(project.vision_path()).unwrap()).unwrap();
        assert_eq!(document["goal"], "Explain clearly");
        assert_eq!(document["pacing"]["avg_shot_seconds"], 1.5);
        assert_eq!(document["pacing"]["max_silence_seconds"], 0.5);
        assert_eq!(document["script_rules"][0], "Explain one concept at a time");
        assert_eq!(document["notes"], "Keep the source assumptions");
        assert_eq!(crate::visions::weights(&document)["diagram"], 0.9);
        assert_eq!(document["references"][0]["note"], "Source");
    }

    #[test]
    fn timebase_labels() {
        assert_eq!(Timebase::FPS_59_94.label(), "59.94");
        assert_eq!(Timebase::FPS_23_976.label(), "23.976");
        assert_eq!(Timebase::FPS_60.label(), "60");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Ep 2 · First Contact"), "ep-2-first-contact");
        assert_eq!(slug("  Hello!! World "), "hello-world");
    }

    #[test]
    fn creates_and_reopens_a_project() {
        let dir = temp_dir("create");
        let root = dir.join("Ep2");
        let created = create_project(&spec(root.clone())).unwrap();

        for p in [
            "project.json",
            "vision.yaml",
            "script/script.md",
            "links.json",
            "A-roll/facecam",
            "B-roll",
            "analysis",
            "sync",
        ] {
            assert!(root.join(p).exists(), "{p} missing");
        }
        let opened = Project::open(&root).unwrap();
        assert_eq!(opened.file, created.file);
        assert_eq!(opened.file.series.as_deref(), Some("Learning to Drift"));
        assert_eq!(
            opened.vision_preset().as_deref(),
            Some("chaotic-gaming-comedy")
        );
        assert!(opened.read_script().unwrap().contains("## Hook"));

        let vision = fs::read_to_string(root.join("vision.yaml")).unwrap();
        assert!(vision.contains("goal: \"Make the crashes funny, then earn the slide.\""));
        assert!(vision.contains("avg_shot_seconds: 1.5"));
    }

    #[test]
    fn refuses_to_overwrite_a_project() {
        let dir = temp_dir("overwrite");
        let root = dir.join("P");
        create_project(&spec(root.clone())).unwrap();
        let err = create_project(&spec(root)).unwrap_err();
        assert!(matches!(err, CoreError::Invalid(_)));
    }

    #[test]
    fn requires_a_name() {
        let mut s = spec(temp_dir("noname").join("P"));
        s.name = "  ".into();
        assert!(matches!(create_project(&s), Err(CoreError::Invalid(_))));
    }

    #[test]
    fn reads_the_sample_project_shape() {
        let json = r#"{
          "$schema": "https://spearfia.dev/schemas/project/v1.json",
          "name": "Sample Project", "spec_version": 1, "adapter": "filmcraft",
          "library_roots": ["E:/SpearFiaLibrary", "./footage"],
          "script": "script/script.md", "vision": "vision.yaml",
          "settings": { "timebase": { "num": 60000, "den": 1001 },
                        "resolution": [1080, 1920], "sample_rate": 48000 }
        }"#;
        let f: ProjectFile = serde_json::from_str(json).unwrap();
        assert_eq!(f.settings.resolution, [1080, 1920]);
        assert_eq!(f.format, Format::LongForm);
        assert!(f.filmcraft.is_none());
    }
    #[test]
    fn creates_a_portable_channel_brief_without_overwriting_existing_guidance() {
        let root = temp_dir("create-channel-project").join("episode");
        let mut request = spec(root.clone());
        request.channel_brief = Some("# Channel: Example\nStyle and lessons".into());
        let project = create_project(&request).unwrap();
        assert_eq!(project.channel_brief().unwrap(), request.channel_brief);
        assert_eq!(
            project.script_approval(None).unwrap(),
            crate::channels::Approval::Draft
        );
        let occupied = temp_dir("occupied-channel-project");
        fs::write(occupied.join("channel.md"), "Existing channel guidance").unwrap();
        request.root = occupied.clone();
        assert!(create_project(&request).is_err());
        assert_eq!(
            fs::read_to_string(occupied.join("channel.md")).unwrap(),
            "Existing channel guidance"
        );
        assert!(!occupied.join("project.json").exists());
        request.root = temp_dir("invalid-channel-project").join("not-created");
        request.channel_brief = Some(String::new());
        assert!(create_project(&request).is_err());
        assert!(!request.root.exists());
    }
}
