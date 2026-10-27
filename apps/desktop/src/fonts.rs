//! Fonts the document can use: the ones bundled with the app, any `.ttf`/`.otf` in the
//! project's `fonts/` folder, and the fonts installed on this computer (scanned in the
//! background the first time the font menu opens).
//!
//! egui has no bold or italic of its own, so every face is its own font family
//! (`font:IBM Plex Sans`, `font:IBM Plex Sans:bold`, ...). A font is loaded the first time a
//! document asks for it; egui applies new fonts on the next frame, so until then the
//! document falls back to IBM Plex Sans.

use egui::{FontData, FontDefinitions, FontFamily};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, OnceLock};

/// The document's default font.
pub const DEFAULT: &str = "IBM Plex Sans";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bundled,
    Project,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Italic,
    Bold,
    BoldItalic,
    SemiBold,
}

impl Face {
    fn suffix(self) -> &'static str {
        match self {
            Face::Regular => "",
            Face::Italic => ":italic",
            Face::Bold => ":bold",
            Face::BoldItalic => ":bolditalic",
            Face::SemiBold => ":semibold",
        }
    }
}

/// The egui family for a face of a font.
pub fn family(name: &str, face: Face) -> FontFamily {
    FontFamily::Name(format!("font:{name}{}", face.suffix()).into())
}

#[derive(Debug, Clone)]
enum Bytes {
    Static(&'static [u8]),
    File(PathBuf),
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    source: Source,
    faces: HashMap<Face, Bytes>,
}

/// The faces a loaded font has (missing ones fall back to regular, with synthetic italic).
#[derive(Debug, Clone, Copy, Default)]
pub struct Faces {
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
    pub semibold: bool,
}

struct Book {
    defs: FontDefinitions,
    fallback: Vec<String>,
    entries: Vec<Entry>,
    /// Font name (lowercase) → (faces, frame it was added).
    loaded: HashMap<String, (Faces, u64)>,
    failed: HashSet<String>,
    scanning: Option<Receiver<Vec<Entry>>>,
    scanned: bool,
    project: Option<PathBuf>,
}

fn book() -> &'static Mutex<Book> {
    static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
    BOOK.get_or_init(|| {
        let defs = FontDefinitions::default();
        let fallback = defs
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        Mutex::new(Book {
            defs,
            fallback,
            entries: bundled(),
            loaded: HashMap::new(),
            failed: HashSet::new(),
            scanning: None,
            scanned: false,
            project: None,
        })
    })
}

fn bundled() -> Vec<Entry> {
    macro_rules! f {
        ($p:literal) => {
            Bytes::Static(include_bytes!(concat!("../assets/fonts/", $p)))
        };
    }
    let e = |name: &str, faces: Vec<(Face, Bytes)>| Entry {
        name: name.to_owned(),
        source: Source::Bundled,
        faces: faces.into_iter().collect(),
    };
    vec![
        e(
            "IBM Plex Sans",
            vec![
                (Face::Regular, f!("IBMPlexSans-Regular.ttf")),
                (Face::Italic, f!("IBMPlexSans-Italic.ttf")),
                (Face::Bold, f!("IBMPlexSans-Bold.ttf")),
                (Face::SemiBold, f!("IBMPlexSans-SemiBold.ttf")),
            ],
        ),
        e(
            "IBM Plex Serif",
            vec![
                (Face::Regular, f!("IBMPlexSerif-Regular.ttf")),
                (Face::Italic, f!("IBMPlexSerif-Italic.ttf")),
                (Face::Bold, f!("IBMPlexSerif-Bold.ttf")),
            ],
        ),
        e(
            "IBM Plex Mono",
            vec![
                (Face::Regular, f!("IBMPlexMono-Regular.ttf")),
                (Face::Bold, f!("IBMPlexMono-Bold.ttf")),
            ],
        ),
        e(
            "Bebas Neue",
            vec![(Face::Regular, f!("BebasNeue-Regular.ttf"))],
        ),
    ]
}

/// Install the default document font. Call once at startup.
pub fn init(ctx: &egui::Context) {
    let mut b = book()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    load(&mut b, ctx, DEFAULT);
    // The shared book may already be loaded by another egui context.
    ctx.set_fonts(b.defs.clone());
    // The default is usable from the first frame.
    if let Some(l) = b.loaded.get_mut(&DEFAULT.to_lowercase()) {
        l.1 = 0;
    }
}

fn load(b: &mut Book, ctx: &egui::Context, name: &str) -> bool {
    let key = name.to_lowercase();
    if b.loaded.contains_key(&key) {
        return true;
    }
    let Some(entry) = b
        .entries
        .iter()
        .find(|e| e.name.to_lowercase() == key)
        .cloned()
    else {
        return false;
    };
    let mut faces = Faces::default();
    let mut any = false;
    for (face, bytes) in &entry.faces {
        let data = match bytes {
            Bytes::Static(s) => FontData::from_static(s),
            Bytes::File(p) => match std::fs::read(p) {
                Ok(v) => FontData::from_owned(v),
                Err(_) => continue,
            },
        };
        let id = format!("font:{}{}", entry.name, face.suffix());
        b.defs.font_data.insert(id.clone(), Arc::new(data));
        let mut list = vec![id];
        list.extend(b.fallback.iter().cloned());
        b.defs.families.insert(family(&entry.name, *face), list);
        any = true;
        match face {
            Face::Bold => faces.bold = true,
            Face::Italic => faces.italic = true,
            Face::BoldItalic => faces.bold_italic = true,
            Face::SemiBold => faces.semibold = true,
            Face::Regular => {}
        }
    }
    if !any || !entry.faces.contains_key(&Face::Regular) {
        b.failed.insert(key);
        return false;
    }
    ctx.set_fonts(b.defs.clone());
    b.loaded.insert(key, (faces, ctx.cumulative_frame_nr()));
    true
}

/// The faces of `name` if it is ready to use this frame. Starts loading it if it isn't;
/// returns `None` until egui has picked it up (or if the font doesn't exist).
pub fn ready(ctx: &egui::Context, name: &str) -> Option<Faces> {
    let mut b = book().lock().ok()?;
    let key = name.to_lowercase();
    if let Some((faces, at)) = b.loaded.get(&key).copied() {
        if ctx.cumulative_frame_nr() > at {
            return Some(faces);
        }
        ctx.request_repaint();
        return None;
    }
    if b.failed.contains(&key) {
        return None;
    }
    if load(&mut b, ctx, name) {
        ctx.request_repaint();
    }
    None
}

/// Whether a font of this name is known (bundled, in the project or installed).
pub fn exists(name: &str) -> bool {
    let key = name.to_lowercase();
    book()
        .lock()
        .map(|b| b.entries.iter().any(|e| e.name.to_lowercase() == key))
        .unwrap_or(false)
}

/// Every known font, A–Z within each source, and whether the system scan is still going.
/// The first call starts scanning the installed fonts.
pub fn list() -> (Vec<(String, Source)>, bool) {
    let Ok(mut b) = book().lock() else {
        return (Vec::new(), false);
    };
    if !b.scanned && b.scanning.is_none() {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(scan_dirs(&system_dirs(), Source::System));
        });
        b.scanning = Some(rx);
    }
    if let Some(rx) = &b.scanning
        && let Ok(found) = rx.try_recv()
    {
        merge(&mut b, found);
        b.scanning = None;
        b.scanned = true;
    }
    let mut out: Vec<(String, Source)> = b
        .entries
        .iter()
        .map(|e| (e.name.clone(), e.source))
        .collect();
    out.sort_by(|a, b| {
        let rank = |s: Source| match s {
            Source::Bundled => 0,
            Source::Project => 1,
            Source::System => 2,
        };
        rank(a.1)
            .cmp(&rank(b.1))
            .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    (out, b.scanning.is_some())
}

/// Use the fonts in `<project>/fonts/`.
pub fn set_project(root: &Path) {
    let dir = root.join("fonts");
    let Ok(mut b) = book().lock() else { return };
    if b.project.as_deref() == Some(dir.as_path()) {
        return;
    }
    b.entries.retain(|e| e.source != Source::Project);
    b.project = Some(dir.clone());
    let found = scan_dirs(&[dir], Source::Project);
    merge(&mut b, found);
}

fn merge(b: &mut Book, found: Vec<Entry>) {
    for e in found {
        let key = e.name.to_lowercase();
        if !b.entries.iter().any(|x| x.name.to_lowercase() == key) {
            b.entries.push(e);
        }
    }
}

fn system_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let mut d = Vec::new();
    if cfg!(windows) {
        let win =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        d.push(win.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            d.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        d.push("/Library/Fonts".into());
        d.push("/System/Library/Fonts".into());
        if let Some(h) = &home {
            d.push(h.join("Library/Fonts"));
        }
    } else {
        d.push("/usr/share/fonts".into());
        d.push("/usr/local/share/fonts".into());
        if let Some(h) = &home {
            d.push(h.join(".local/share/fonts"));
            d.push(h.join(".fonts"));
        }
    }
    d
}

/// Read the family name and face of every `.ttf` / `.otf` under `dirs`.
fn scan_dirs(dirs: &[PathBuf], source: Source) -> Vec<Entry> {
    use skrifa::MetadataProvider;
    use skrifa::attribute::Style;
    use skrifa::string::StringId;
    let mut files = Vec::new();
    for d in dirs {
        walk(d, 0, &mut files);
    }
    let mut by_name: HashMap<String, Entry> = HashMap::new();
    for path in files {
        // Very large files are CJK collections; skip them to keep the scan quick.
        if std::fs::metadata(&path)
            .map(|m| m.len() > 25_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(font) = skrifa::FontRef::new(&bytes) else {
            continue;
        };
        let name = font
            .localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
            .english_or_first()
            .or_else(|| {
                font.localized_strings(StringId::FAMILY_NAME)
                    .english_or_first()
            })
            .map(|s| s.to_string());
        let Some(name) = name.filter(|n| !n.is_empty() && !n.starts_with('.')) else {
            continue;
        };
        let a = font.attributes();
        let italic = !matches!(a.style, Style::Normal);
        let w = a.weight.value();
        let face = match (w >= 650.0, italic) {
            (true, true) => Face::BoldItalic,
            (true, false) => Face::Bold,
            (false, true) => Face::Italic,
            (false, false) if w >= 550.0 => Face::SemiBold,
            (false, false) => Face::Regular,
        };
        // Prefer the face closest to normal weight for Regular.
        let e = by_name.entry(name.to_lowercase()).or_insert_with(|| Entry {
            name: name.clone(),
            source,
            faces: HashMap::new(),
        });
        let better = face != Face::Regular
            || (w - 400.0).abs() < 1.0
            || !e.faces.contains_key(&Face::Regular);
        if better {
            e.faces
                .entry(face)
                .or_insert_with(|| Bytes::File(path.clone()));
        }
    }
    // A family with no upright face uses its first face as regular.
    by_name
        .into_values()
        .filter_map(|mut e| {
            if !e.faces.contains_key(&Face::Regular) {
                let any = e.faces.values().next().cloned()?;
                e.faces.insert(Face::Regular, any);
            }
            Some(e)
        })
        .collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 4 {
                walk(&p, depth + 1, out);
            }
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("ttf") || x.eq_ignore_ascii_case("otf"))
        {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_read_back_their_names() {
        let dir = std::env::temp_dir().join(format!("spearfia-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.ttf"),
            include_bytes!("../assets/fonts/IBMPlexSerif-Italic.ttf"),
        )
        .unwrap();
        std::fs::write(
            dir.join("b.ttf"),
            include_bytes!("../assets/fonts/IBMPlexSerif-Bold.ttf"),
        )
        .unwrap();
        std::fs::write(
            dir.join("c.ttf"),
            include_bytes!("../assets/fonts/BebasNeue-Regular.ttf"),
        )
        .unwrap();
        let mut found = scan_dirs(&[dir], Source::Project);
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found[0].name, "Bebas Neue");
        assert_eq!(found[1].name, "IBM Plex Serif");
        assert!(found[1].faces.contains_key(&Face::Italic));
        assert!(found[1].faces.contains_key(&Face::Bold));
        assert!(
            found[1].faces.contains_key(&Face::Regular),
            "falls back to a face as regular"
        );
    }
    #[test]
    fn every_new_context_gets_the_cached_default_font() {
        for _ in 0..2 {
            let ctx = egui::Context::default();
            init(&ctx);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.label(
                    egui::RichText::new("Second context").family(family(DEFAULT, Face::SemiBold)),
                );
            });
            output.textures_delta.clear();
        }
    }
}
