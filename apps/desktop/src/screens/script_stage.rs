//! Script stage: a word-processor page for `script.md`, laid out like Google Docs.
//!
//! - Header: document title, save status, menus (File, Edit, View, Insert, Format, Cues,
//!   Tools, Help), the comments toggle and the Editing / Suggesting / Viewing mode.
//! - Toolbar: search menus, undo/redo, zoom, paragraph style, bold/italic/underline/strike,
//!   link, comment, lists, indent, clear formatting, and buttons that insert cues.
//! - Document tabs on the left (the `.md` files in `script/`), with the open tab's summary
//!   and heading outline.
//! - The page: type directly on it. Markdown syntax hides except on the caret's line (see
//!   [`crate::doc`]). Typing `/` opens an insert menu.
//! - Comments and suggestions in the right margin, level with their text.
//! - Status bar: page, words, spoken runtime, line/column and Paged / Pageless / Markdown.
//!
//! Everything is stored in plain Markdown; edits autosave about a second after typing stops.

use crate::doc::{self, Look, Mark};
use crate::icons::{self, Icon};
use crate::script_view::{self, Selection};
use crate::theme;
use crate::widgets::{self, mono, muted, strong, text};
use egui::text::CCursor;
use egui::text_selection::CCursorRange;
use egui::widgets::text_edit::TextEditState;
use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Key, Layout, Margin,
    Modifiers, Panel, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2,
};
use spearfia_core::comments::{self, Comment, Comments, Reply};
use spearfia_core::links::Links;
use spearfia_core::project::Project;
use spearfia_core::tabs::Tab;
use spearfia_script::format::{self, List, Style};
use spearfia_script::{Block, Script, edit};
use std::collections::HashMap;
use std::ops::Range;

/// Something the workspace should do for the Script page.
pub enum ScriptEvent {
    /// Open the Media stage's script view with these words highlighted, ready for a clip.
    LinkClip(Selection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Paged,
    Pageless,
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Editing,
    Suggesting,
    Viewing,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Editing => "Editing",
            Self::Suggesting => "Suggesting",
            Self::Viewing => "Viewing",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Editing => "Edit the script directly",
            Self::Suggesting => "Select text to suggest a change; the text stays as it is",
            Self::Viewing => "Read or comment",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Editing => Icon::Pencil,
            Self::Suggesting => Icon::Chat,
            Self::Viewing => Icon::Eye,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CueKind {
    Sfx,
    Music,
    Look,
    Clip,
}

impl CueKind {
    fn name(self) -> &'static str {
        match self {
            Self::Sfx => "sfx",
            Self::Music => "music",
            Self::Look => "look",
            Self::Clip => "clip",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sfx => "SFX",
            Self::Music => "Music",
            Self::Look => "Look",
            Self::Clip => "Clip",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Sfx => Icon::Sfx,
            Self::Music => Icon::Music,
            Self::Look => Icon::Look,
            Self::Clip => Icon::Clip,
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::Sfx => "s",
            Self::Music => "m",
            Self::Look => "l",
            Self::Clip => "c",
        }
    }
}

/// Every action the menus, toolbar, shortcuts and menu search can run.
#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    Save,
    Reload,
    NewTab,
    OpenFolder,
    Undo,
    Redo,
    Find,
    SelectAll,
    Bold,
    Italic,
    Underline,
    Strike,
    Link,
    Comment,
    Suggest,
    Style(Style),
    List(List),
    Indent,
    Outdent,
    Clear,
    Cue(CueKind),
    ChapterCard,
    Divider,
    Table,
    View(View),
    Zoom(f32),
    ToggleTabs,
    ToggleComments,
    Mode(Mode),
    Help,
    WordCount,
    LinkClip,
    NextMissingClip,
    Font(String),
    Size(f32),
    SizeStep(f32),
    Color(Option<String>),
    Highlight(Option<String>),
    Align(doc::HAlign),
    Spacing(f32),
    UpdateStyle,
    ResetStyle,
    Styles,
}

impl Cmd {
    fn label(&self) -> String {
        match self {
            Cmd::Save => "Save".into(),
            Cmd::Reload => "Reload from disk".into(),
            Cmd::NewTab => "New tab…".into(),
            Cmd::OpenFolder => "Show script folder".into(),
            Cmd::Undo => "Undo".into(),
            Cmd::Redo => "Redo".into(),
            Cmd::Find => "Find and replace".into(),
            Cmd::SelectAll => "Select all".into(),
            Cmd::Bold => "Bold".into(),
            Cmd::Italic => "Italic".into(),
            Cmd::Underline => "Underline".into(),
            Cmd::Strike => "Strikethrough".into(),
            Cmd::Link => "Insert link".into(),
            Cmd::Comment => "Comment".into(),
            Cmd::Suggest => "Suggest an edit".into(),
            Cmd::Style(s) => format!("Style: {}", s.label()),
            Cmd::List(List::Bullet) => "Bulleted list".into(),
            Cmd::List(List::Numbered) => "Numbered list".into(),
            Cmd::List(List::Check) => "Checklist".into(),
            Cmd::Indent => "Increase indent".into(),
            Cmd::Outdent => "Decrease indent".into(),
            Cmd::Clear => "Clear formatting".into(),
            Cmd::Cue(k) => format!("Insert {} cue", k.label()),
            Cmd::ChapterCard => "Insert chapter card".into(),
            Cmd::Divider => "Insert transition divider".into(),
            Cmd::Table => "Insert table".into(),
            Cmd::View(View::Paged) => "Paged view".into(),
            Cmd::View(View::Pageless) => "Pageless view".into(),
            Cmd::View(View::Markdown) => "Markdown source".into(),
            Cmd::Zoom(z) => format!("Zoom {:.0}%", z * 100.0),
            Cmd::ToggleTabs => "Show document tabs".into(),
            Cmd::ToggleComments => "Show comments".into(),
            Cmd::Mode(m) => format!("Mode: {}", m.label()),
            Cmd::Help => "Script syntax".into(),
            Cmd::WordCount => "Word count".into(),
            Cmd::LinkClip => "Link a clip to the selection".into(),
            Cmd::NextMissingClip => "Next on-screen line without a clip".into(),
            Cmd::Font(f) => format!("Font: {f}"),
            Cmd::Size(v) => format!("Font size {v}"),
            Cmd::SizeStep(d) if *d > 0.0 => "Increase font size".into(),
            Cmd::SizeStep(_) => "Decrease font size".into(),
            Cmd::Color(_) => "Text color".into(),
            Cmd::Highlight(_) => "Highlight color".into(),
            Cmd::Align(a) => format!("Align {}", a.key()),
            Cmd::Spacing(v) => format!("Line spacing {v}"),
            Cmd::UpdateStyle => "Update style to match".into(),
            Cmd::ResetStyle => "Reset style".into(),
            Cmd::Styles => "Paragraph styles…".into(),
        }
    }

    fn shortcut(&self) -> &'static str {
        match self {
            Cmd::Save => "Ctrl+S",
            Cmd::Undo => "Ctrl+Z",
            Cmd::Redo => "Ctrl+Y",
            Cmd::Find => "Ctrl+F",
            Cmd::SelectAll => "Ctrl+A",
            Cmd::Bold => "Ctrl+B",
            Cmd::Italic => "Ctrl+I",
            Cmd::Underline => "Ctrl+U",
            Cmd::Strike => "Alt+Shift+5",
            Cmd::Link => "Ctrl+K",
            Cmd::Comment => "Ctrl+Alt+M",
            Cmd::Style(s) => s.shortcut(),
            Cmd::List(List::Numbered) => "Ctrl+Shift+7",
            Cmd::List(List::Bullet) => "Ctrl+Shift+8",
            Cmd::List(List::Check) => "Ctrl+Shift+9",
            Cmd::Indent => "Ctrl+]",
            Cmd::Outdent => "Ctrl+[",
            Cmd::Clear => "Ctrl+\\",
            Cmd::Divider => "/divider",
            Cmd::SizeStep(d) if *d > 0.0 => "Ctrl+Shift+.",
            Cmd::SizeStep(_) => "Ctrl+Shift+,",
            Cmd::Align(doc::HAlign::Left) => "Ctrl+Shift+L",
            Cmd::Align(doc::HAlign::Center) => "Ctrl+Shift+E",
            Cmd::Align(doc::HAlign::Right) => "Ctrl+Shift+R",
            _ => "",
        }
    }

    /// Changes the text (so not allowed while suggesting or viewing).
    fn edits(&self) -> bool {
        matches!(
            self,
            Cmd::Undo
                | Cmd::Redo
                | Cmd::Bold
                | Cmd::Italic
                | Cmd::Underline
                | Cmd::Strike
                | Cmd::Link
                | Cmd::Style(_)
                | Cmd::List(_)
                | Cmd::Indent
                | Cmd::Outdent
                | Cmd::Clear
                | Cmd::Cue(_)
                | Cmd::ChapterCard
                | Cmd::Divider
                | Cmd::Table
                | Cmd::Font(_)
                | Cmd::Size(_)
                | Cmd::SizeStep(_)
                | Cmd::Color(_)
                | Cmd::Highlight(_)
                | Cmd::Align(_)
                | Cmd::Spacing(_)
                | Cmd::UpdateStyle
                | Cmd::ResetStyle
        )
    }
}

/// Everything listed in the menus, for "Search menus".
fn all_cmds() -> Vec<Cmd> {
    let mut v = vec![
        Cmd::Save,
        Cmd::Reload,
        Cmd::NewTab,
        Cmd::OpenFolder,
        Cmd::Undo,
        Cmd::Redo,
        Cmd::Find,
        Cmd::SelectAll,
        Cmd::Bold,
        Cmd::Italic,
        Cmd::Underline,
        Cmd::Strike,
        Cmd::Link,
        Cmd::Comment,
        Cmd::Suggest,
    ];
    v.extend(Style::ALL.map(Cmd::Style));
    v.extend([
        Cmd::List(List::Bullet),
        Cmd::List(List::Numbered),
        Cmd::List(List::Check),
        Cmd::Indent,
        Cmd::Outdent,
        Cmd::Clear,
        Cmd::Align(doc::HAlign::Left),
        Cmd::Align(doc::HAlign::Center),
        Cmd::Align(doc::HAlign::Right),
        Cmd::Spacing(1.0),
        Cmd::Spacing(1.15),
        Cmd::Spacing(1.5),
        Cmd::Spacing(2.0),
        Cmd::SizeStep(1.0),
        Cmd::SizeStep(-1.0),
        Cmd::Color(None),
        Cmd::Highlight(None),
        Cmd::UpdateStyle,
        Cmd::ResetStyle,
        Cmd::Styles,
    ]);
    v.extend([CueKind::Sfx, CueKind::Music, CueKind::Look, CueKind::Clip].map(Cmd::Cue));
    v.extend([
        Cmd::ChapterCard,
        Cmd::Divider,
        Cmd::Table,
        Cmd::View(View::Paged),
        Cmd::View(View::Pageless),
        Cmd::View(View::Markdown),
        Cmd::ToggleTabs,
        Cmd::ToggleComments,
        Cmd::Mode(Mode::Editing),
        Cmd::Mode(Mode::Suggesting),
        Cmd::Mode(Mode::Viewing),
        Cmd::WordCount,
        Cmd::LinkClip,
        Cmd::NextMissingClip,
        Cmd::Help,
    ]);
    v
}

/// One open document (a tab).
struct Buf {
    rel: String,
    main: bool,
    source: String,
    parsed: Script,
    dirty: bool,
    last_edit: f64,
    error: Option<String>,
    /// The file couldn't be read, so `source` is empty, not the document: never save it.
    load_failed: bool,
}

impl Buf {
    fn load(project: &Project, rel: &str, main: bool) -> Self {
        let (source, error) = match project.read_doc(rel) {
            Ok(s) => (s, None),
            Err(e) => (String::new(), Some(e.to_string())),
        };
        Self {
            rel: rel.to_owned(),
            main,
            parsed: spearfia_script::parse(&source),
            source,
            dirty: false,
            last_edit: 0.0,
            load_failed: error.is_some(),
            error,
        }
    }

    fn changed(&mut self, now: f64) {
        self.parsed = spearfia_script::parse(&self.source);
        self.dirty = true;
        self.last_edit = now;
    }

    fn save(&mut self, project: &Project) {
        if self.load_failed {
            self.error = Some(format!(
                "Not saved: {} couldn't be read when it was opened. Fix it, then File → Reload.",
                self.file_name()
            ));
            return;
        }
        match project.write_doc(&self.rel, &self.source) {
            Ok(()) => {
                self.dirty = false;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Couldn't save: {e}")),
        }
    }

    fn file_name(&self) -> &str {
        self.rel.rsplit('/').next().unwrap_or(&self.rel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposerKind {
    Comment,
    Suggest,
}

struct Composer {
    kind: ComposerKind,
    range: Range<usize>,
    quote: String,
    line: usize,
    text: String,
}

struct Slash {
    /// Byte offset of the `/`.
    start: usize,
    query: String,
    selected: usize,
}

#[derive(Debug, Clone, Copy)]
enum SlashAction {
    Cmd(CueKind),
    Chapter,
    Divider,
    Style(Style),
    List(List),
    Table,
    Comment,
}

struct SlashItem {
    label: &'static str,
    hint: &'static str,
    icon: Icon,
    color: Color32,
    action: SlashAction,
}

fn slash_items() -> Vec<SlashItem> {
    use SlashAction as A;
    let i = |label, hint, icon, color, action| SlashItem {
        label,
        hint,
        icon,
        color,
        action,
    };
    vec![
        i(
            "Chapter card",
            "template look",
            Icon::Look,
            theme::CUE_LOOK,
            A::Chapter,
        ),
        i(
            "Transition divider",
            "between lines",
            Icon::Transition,
            theme::CUE_TRANSITION,
            A::Divider,
        ),
        i(
            "Sound effect",
            "stinger on this word",
            Icon::Sfx,
            theme::CUE_SFX,
            A::Cmd(CueKind::Sfx),
        ),
        i(
            "Music cue",
            "starts here",
            Icon::Music,
            theme::CUE_MUSIC,
            A::Cmd(CueKind::Music),
        ),
        i(
            "Clip",
            "footage for this line",
            Icon::Clip,
            theme::CUE_CLIP,
            A::Cmd(CueKind::Clip),
        ),
        i(
            "Look",
            "facecam, overlays",
            Icon::Look,
            theme::CUE_LOOK,
            A::Cmd(CueKind::Look),
        ),
        i(
            "VO line",
            "spoken, voiceover",
            Icon::Chat,
            theme::CUE_VO,
            A::Style(Style::Vo),
        ),
        i(
            "Live line",
            "spoken to camera",
            Icon::Chat,
            theme::CUE_LIVE,
            A::Style(Style::Live),
        ),
        i(
            "On-screen",
            "what we see",
            Icon::Eye,
            theme::TEXT_2,
            A::Style(Style::OnScreen),
        ),
        i(
            "Heading",
            "new section",
            Icon::Doc,
            theme::TEXT_2,
            A::Style(Style::Heading1),
        ),
        i(
            "Note",
            "not spoken",
            Icon::Comment,
            theme::TEXT_2,
            A::Style(Style::Note),
        ),
        i(
            "Checklist",
            "",
            Icon::Checklist,
            theme::TEXT_2,
            A::List(List::Check),
        ),
        i(
            "Bulleted list",
            "",
            Icon::Bullets,
            theme::TEXT_2,
            A::List(List::Bullet),
        ),
        i("Table", "", Icon::Table, theme::TEXT_2, A::Table),
        i(
            "Comment",
            "on this line",
            Icon::Comment,
            theme::ACCENT,
            A::Comment,
        ),
    ]
}

pub struct ScriptStage {
    main: Buf,
    side: Option<Buf>,
    tabs: Vec<Tab>,
    view: View,
    mode: Mode,
    zoom: f32,
    show_tabs: bool,
    show_comments: bool,
    comments: Comments,
    comments_error: Option<String>,
    composer: Option<Composer>,
    reply_drafts: HashMap<String, String>,
    active_comment: Option<String>,
    slash: Option<Slash>,
    find: Option<(String, String, usize)>,
    link_prompt: Option<(Range<usize>, String)>,
    new_tab: Option<String>,
    help: bool,
    word_count: bool,
    menu_search: String,
    title_edit: Option<String>,
    /// Selection to put in the editor next frame (bytes).
    pending_sel: Option<Range<usize>>,
    /// The editor's selection as of last frame (bytes), kept while toolbar buttons have focus.
    sel: Range<usize>,
    caret: usize,
    scroll_to: Option<usize>,
    author: String,
    event: Option<ScriptEvent>,
    pages: usize,
    page_of_caret: usize,
    /// Card heights from last frame, so a card's click area can sit under its buttons.
    card_h: HashMap<String, f32>,
    styles_open: bool,
    font_filter: String,
    hex_draft: String,
    size_draft: Option<String>,
}

impl ScriptStage {
    pub fn load(project: &Project) -> Self {
        crate::fonts::set_project(&project.root);
        let main_rel = project.file.script.replace('\\', "/");
        let (comments, comments_error) = match Comments::load(project) {
            Ok(c) => (c, None),
            Err(e) => (Comments::default(), Some(e.to_string())),
        };
        Self {
            main: Buf::load(project, &main_rel, true),
            side: None,
            tabs: project.tabs(),
            view: View::Paged,
            mode: Mode::Editing,
            zoom: 1.0,
            show_tabs: true,
            show_comments: true,
            comments,
            comments_error,
            composer: None,
            reply_drafts: HashMap::new(),
            active_comment: None,
            slash: None,
            find: None,
            link_prompt: None,
            new_tab: None,
            help: false,
            word_count: false,
            menu_search: String::new(),
            title_edit: None,
            pending_sel: None,
            sel: 0..0,
            caret: 0,
            scroll_to: None,
            author: author(),
            event: None,
            pages: 1,
            page_of_caret: 1,
            card_h: HashMap::new(),
            styles_open: false,
            font_filter: String::new(),
            hex_draft: String::new(),
            size_draft: None,
        }
    }

    /// The episode script as text, including edits not yet autosaved.
    pub fn source(&self) -> &str {
        &self.main.source
    }

    /// The parsed episode script, for other stages.
    pub fn script(&self) -> &Script {
        &self.main.parsed
    }

    /// Save the episode script now if it has unsaved typing, so a command built from the
    /// file on disk sees what the user sees.
    pub fn flush(&mut self, project: &Project) -> Result<(), String> {
        if self.main.dirty {
            self.main.save(project);
        }
        self.main.error.clone().map_or(Ok(()), Err)
    }

    /// Take text the command layer has already written to disk (no save needed).
    pub fn adopt(&mut self, text: String) {
        self.main.source = text;
        self.main.parsed = spearfia_script::parse(&self.main.source);
        self.main.dirty = false;
    }

    /// Replace the episode script with an edit made elsewhere (e.g. a cue added on the
    /// Music stage) and save it.
    pub fn apply_edit(&mut self, project: &Project, source: String) -> Result<(), String> {
        self.main.source = source;
        self.main.parsed = spearfia_script::parse(&self.main.source);
        self.main.save(project);
        match &self.main.error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    fn buf(&self) -> &Buf {
        self.side.as_ref().unwrap_or(&self.main)
    }

    fn buf_mut(&mut self) -> &mut Buf {
        self.side.as_mut().unwrap_or(&mut self.main)
    }

    fn editor_id(&self) -> Id {
        Id::new(("script-editor", self.buf().rel.clone()))
    }

    fn can_edit(&self) -> bool {
        self.mode == Mode::Editing
    }

    pub fn show(
        &mut self,
        project: &Project,
        links: &Links,
        ui: &mut egui::Ui,
    ) -> Option<ScriptEvent> {
        let now = ui.input(|i| i.time);
        // Autosave a second after typing stops.
        for b in std::iter::once(&mut self.main).chain(self.side.as_mut()) {
            if b.dirty && now - b.last_edit > 1.0 {
                b.save(project);
            }
        }
        if self.buf().dirty {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(400));
        }

        let mut cmds: Vec<Cmd> = Vec::new();
        self.shortcuts(ui, &mut cmds);

        Panel::top("doc-header")
            .exact_size(64.0)
            .show_separator_line(false)
            .frame(widgets::panel(theme::BG_PANEL).inner_margin(Margin::symmetric(14, 6)))
            .show(ui, |ui| self.header(ui, project, &mut cmds));
        Panel::top("doc-toolbar")
            .exact_size(46.0)
            .show_separator_line(false)
            .frame(widgets::panel(theme::BG_PANEL).inner_margin(Margin::symmetric(12, 5)))
            .show(ui, |ui| self.toolbar(ui, &mut cmds));
        Panel::bottom("doc-status")
            .exact_size(26.0)
            .frame(widgets::panel(theme::BG_PANEL).inner_margin(Margin::symmetric(14, 4)))
            .show(ui, |ui| self.status(ui, project, &mut cmds));
        if self.show_tabs {
            Panel::left("doc-tabs")
                .exact_size(256.0)
                .frame(widgets::panel(theme::BG_PANEL).inner_margin(Margin::symmetric(12, 12)))
                .show(ui, |ui| self.tabs_panel(ui, project));
        }
        egui::CentralPanel::default()
            .frame(widgets::panel(theme::BG_INSET))
            .show(ui, |ui| match self.view {
                View::Markdown => self.markdown(ui),
                _ => self.canvas(ui, project, links, &mut cmds),
            });

        self.windows(ui.ctx(), project, &mut cmds);
        for c in cmds {
            self.run(ui.ctx(), project, c);
        }
        self.event.take()
    }

    // ---------------------------------------------------------------- input

    fn shortcuts(&mut self, ui: &egui::Ui, cmds: &mut Vec<Cmd>) {
        let focused = ui.ctx().memory(|m| m.has_focus(self.editor_id()));
        let take = |m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
        if take(Modifiers::COMMAND, Key::S) {
            cmds.push(Cmd::Save);
        }
        if take(Modifiers::COMMAND, Key::F) || take(Modifiers::COMMAND, Key::H) {
            cmds.push(Cmd::Find);
        }
        if !focused {
            return;
        }
        let ctrl_alt = Modifiers::COMMAND | Modifiers::ALT;
        let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        for (m, k, c) in [
            (Modifiers::COMMAND, Key::B, Cmd::Bold),
            (Modifiers::COMMAND, Key::I, Cmd::Italic),
            (Modifiers::COMMAND, Key::U, Cmd::Underline),
            (Modifiers::ALT | Modifiers::SHIFT, Key::Num5, Cmd::Strike),
            (Modifiers::COMMAND, Key::K, Cmd::Link),
            (ctrl_alt, Key::M, Cmd::Comment),
            (ctrl_shift, Key::Num7, Cmd::List(List::Numbered)),
            (ctrl_shift, Key::Num8, Cmd::List(List::Bullet)),
            (ctrl_shift, Key::Num9, Cmd::List(List::Check)),
            (Modifiers::COMMAND, Key::CloseBracket, Cmd::Indent),
            (Modifiers::COMMAND, Key::OpenBracket, Cmd::Outdent),
            (Modifiers::COMMAND, Key::Backslash, Cmd::Clear),
            (ctrl_alt, Key::T, Cmd::Style(Style::Title)),
            (ctrl_alt, Key::Num1, Cmd::Style(Style::Heading1)),
            (ctrl_alt, Key::Num2, Cmd::Style(Style::Heading2)),
            (ctrl_alt, Key::Num3, Cmd::Style(Style::Heading3)),
            (ctrl_alt, Key::Num0, Cmd::Style(Style::Vo)),
            (ctrl_alt, Key::L, Cmd::Style(Style::Live)),
            (ctrl_alt, Key::O, Cmd::Style(Style::OnScreen)),
            (ctrl_alt, Key::N, Cmd::Style(Style::Note)),
            (ctrl_shift, Key::Period, Cmd::SizeStep(1.0)),
            (ctrl_shift, Key::Comma, Cmd::SizeStep(-1.0)),
            (ctrl_shift, Key::L, Cmd::Align(doc::HAlign::Left)),
            (ctrl_shift, Key::E, Cmd::Align(doc::HAlign::Center)),
            (ctrl_shift, Key::R, Cmd::Align(doc::HAlign::Right)),
        ] {
            if take(m, k) {
                cmds.push(c);
            }
        }
    }

    // --------------------------------------------------------------- header

    fn header(&mut self, ui: &mut egui::Ui, project: &Project, cmds: &mut Vec<Cmd>) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(30.0, 40.0), Sense::hover());
            icons::paint(
                ui.painter(),
                Rect::from_center_size(r.center(), vec2(26.0, 26.0)),
                Icon::Doc,
                theme::ACCENT,
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    self.title(ui, project);
                    let b = self.buf();
                    let (status, color) = match (&b.error, b.dirty) {
                        (Some(e), _) => (e.clone(), theme::DANGER),
                        (None, true) => ("Saving…".to_owned(), theme::TEXT_MUTED),
                        (None, false) => (format!("Saved · {}", b.file_name()), theme::TEXT_MUTED),
                    };
                    ui.label(text(status, 11.5, color));
                });
                self.menus(ui, cmds);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Mode menu.
                let mode = self.mode;
                let g = ui.painter().layout_no_wrap(
                    mode.label().to_owned(),
                    FontId::proportional(12.5),
                    theme::TEXT,
                );
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(g.size().x + 58.0, 30.0), Sense::click());
                {
                    let p = ui.painter();
                    p.rect(
                        rect,
                        15.0,
                        if resp.hovered() {
                            theme::BG_HOVER
                        } else {
                            Color32::TRANSPARENT
                        },
                        Stroke::new(1.0, theme::BORDER_STRONG),
                        egui::StrokeKind::Inside,
                    );
                    icons::paint(
                        p,
                        Rect::from_center_size(
                            pos2(rect.left() + 18.0, rect.center().y),
                            vec2(15.0, 15.0),
                        ),
                        mode.icon(),
                        theme::TEXT,
                    );
                    p.galley(
                        pos2(rect.left() + 32.0, rect.center().y - g.size().y / 2.0),
                        g,
                        theme::TEXT,
                    );
                    icons::paint(
                        p,
                        Rect::from_center_size(
                            pos2(rect.right() - 14.0, rect.center().y),
                            vec2(12.0, 12.0),
                        ),
                        Icon::Down,
                        theme::TEXT_2,
                    );
                }
                egui::Popup::menu(&resp).show(|ui| {
                    for m in [Mode::Editing, Mode::Suggesting, Mode::Viewing] {
                        let r = ui.add(egui::Button::selectable(
                            self.mode == m,
                            RichText::new(format!("{}\n{}", m.label(), m.hint())).size(12.5),
                        ));
                        if r.clicked() {
                            cmds.push(Cmd::Mode(m));
                        }
                    }
                });
                // Comments toggle.
                let open = self.comments.open_on(&self.buf().rel).count();
                let r = icons::button(
                    ui,
                    Icon::Chat,
                    &format!("Comments ({open})"),
                    true,
                    self.show_comments,
                );
                if open > 0 {
                    ui.painter().text(
                        r.rect.right_top() + vec2(-2.0, 4.0),
                        Align2::CENTER_CENTER,
                        open.to_string(),
                        FontId::proportional(9.5),
                        theme::ACCENT,
                    );
                }
                if r.clicked() {
                    cmds.push(Cmd::ToggleComments);
                }
            });
        });
    }

    fn title(&mut self, ui: &mut egui::Ui, project: &Project) {
        let b = self.buf();
        if !b.main {
            ui.label(text(
                b.rel
                    .rsplit('/')
                    .next()
                    .unwrap_or(&b.rel)
                    .trim_end_matches(".md"),
                16.0,
                theme::TEXT,
            ));
            return;
        }
        let current = b
            .parsed
            .front_matter
            .as_ref()
            .and_then(|f| f.title.clone())
            .unwrap_or_else(|| project.name().to_owned());
        let can_edit = self.can_edit();
        let editing = self.title_edit.get_or_insert_with(|| current.clone());
        let width = (editing.chars().count() as f32 * 9.0 + 20.0).clamp(80.0, 420.0);
        let r = ui.add(
            egui::TextEdit::singleline(editing)
                .frame(Frame::NONE)
                .font(FontId::proportional(16.0))
                .desired_width(width)
                .text_color(theme::TEXT),
        );
        if !r.has_focus() {
            let edited = editing.trim().to_owned();
            if r.lost_focus() && edited != current && can_edit {
                let new = set_front_matter(&self.main.source, "title", &edited);
                self.main.source = new;
                let now = ui.input(|i| i.time);
                self.main.changed(now);
            }
            self.title_edit = None;
        }
        r.on_hover_text("Rename (front matter title)");
    }

    fn menus(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let editing = self.can_edit();
        let item = |ui: &mut egui::Ui, c: Cmd, cmds: &mut Vec<Cmd>, enabled: bool| {
            let r = ui.add_enabled(
                enabled,
                egui::Button::new(c.label()).shortcut_text(c.shortcut()),
            );
            if r.clicked() {
                cmds.push(c);
                ui.close();
            }
        };
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                item(ui, Cmd::Save, cmds, true);
                item(ui, Cmd::Reload, cmds, true);
                ui.separator();
                item(ui, Cmd::NewTab, cmds, true);
                item(ui, Cmd::OpenFolder, cmds, true);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, Cmd::Undo, cmds, editing);
                item(ui, Cmd::Redo, cmds, editing);
                ui.separator();
                item(ui, Cmd::SelectAll, cmds, true);
                item(ui, Cmd::Find, cmds, true);
            });
            ui.menu_button("View", |ui| {
                for v in [View::Paged, View::Pageless, View::Markdown] {
                    if ui.radio(self.view == v, Cmd::View(v).label()).clicked() {
                        cmds.push(Cmd::View(v));
                        ui.close();
                    }
                }
                ui.separator();
                for z in [0.75, 0.9, 1.0, 1.25, 1.5] {
                    if ui
                        .radio((self.zoom - z).abs() < 0.01, format!("{:.0}%", z * 100.0))
                        .clicked()
                    {
                        cmds.push(Cmd::Zoom(z));
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.show_tabs.clone(), "Document tabs")
                    .clicked()
                {
                    cmds.push(Cmd::ToggleTabs);
                }
                if ui
                    .checkbox(&mut self.show_comments.clone(), "Comments")
                    .clicked()
                {
                    cmds.push(Cmd::ToggleComments);
                }
            });
            ui.menu_button("Insert", |ui| {
                for k in [CueKind::Sfx, CueKind::Music, CueKind::Look, CueKind::Clip] {
                    item(ui, Cmd::Cue(k), cmds, editing);
                }
                item(ui, Cmd::ChapterCard, cmds, editing);
                item(ui, Cmd::Divider, cmds, editing);
                ui.separator();
                item(ui, Cmd::Link, cmds, editing);
                item(ui, Cmd::Table, cmds, editing);
                item(ui, Cmd::List(List::Check), cmds, editing);
                ui.separator();
                item(ui, Cmd::Comment, cmds, true);
            });
            ui.menu_button("Format", |ui| {
                ui.menu_button("Paragraph style", |ui| {
                    for s in Style::ALL {
                        item(ui, Cmd::Style(s), cmds, editing);
                    }
                });
                item(ui, Cmd::UpdateStyle, cmds, editing);
                item(ui, Cmd::ResetStyle, cmds, editing);
                item(ui, Cmd::Styles, cmds, true);
                ui.separator();
                for c in [Cmd::Bold, Cmd::Italic, Cmd::Underline, Cmd::Strike] {
                    item(ui, c, cmds, editing);
                }
                ui.menu_button("Align", |ui| {
                    for a in [doc::HAlign::Left, doc::HAlign::Center, doc::HAlign::Right] {
                        item(ui, Cmd::Align(a), cmds, editing);
                    }
                });
                ui.menu_button("Line spacing", |ui| {
                    for v in [1.0, 1.15, 1.5, 2.0] {
                        item(ui, Cmd::Spacing(v), cmds, editing);
                    }
                });
                ui.menu_button("Font size", |ui| {
                    item(ui, Cmd::SizeStep(1.0), cmds, editing);
                    item(ui, Cmd::SizeStep(-1.0), cmds, editing);
                });
                ui.separator();
                for c in [
                    Cmd::List(List::Bullet),
                    Cmd::List(List::Numbered),
                    Cmd::List(List::Check),
                    Cmd::Indent,
                    Cmd::Outdent,
                ] {
                    item(ui, c, cmds, editing);
                }
                ui.separator();
                item(ui, Cmd::Clear, cmds, editing);
            });
            ui.menu_button("Cues", |ui| {
                for k in [CueKind::Sfx, CueKind::Music, CueKind::Look, CueKind::Clip] {
                    item(ui, Cmd::Cue(k), cmds, editing);
                }
                item(ui, Cmd::Divider, cmds, editing);
                ui.separator();
                item(ui, Cmd::LinkClip, cmds, self.buf().main);
                item(ui, Cmd::NextMissingClip, cmds, self.buf().main);
            });
            ui.menu_button("Tools", |ui| {
                item(ui, Cmd::WordCount, cmds, true);
                item(ui, Cmd::Find, cmds, true);
            });
            ui.menu_button("Help", |ui| {
                item(ui, Cmd::Help, cmds, true);
            });
        });
    }

    // -------------------------------------------------------------- toolbar

    fn toolbar(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let editing = self.can_edit();
        let sep = |ui: &mut egui::Ui| {
            let (r, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
            ui.painter().line_segment(
                [r.center_top(), r.center_bottom()],
                Stroke::new(1.0, theme::BORDER),
            );
        };
        Frame::new()
            .fill(theme::BG_RAISED)
            .corner_radius(CornerRadius::same(18))
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    self.menu_search(ui, cmds);
                    ui.add_space(4.0);
                    for (ic, c, tip) in [
                        (Icon::Undo, Cmd::Undo, "Undo (Ctrl+Z)"),
                        (Icon::Redo, Cmd::Redo, "Redo (Ctrl+Y)"),
                    ] {
                        if icons::button(ui, ic, tip, editing, false).clicked() {
                            cmds.push(c);
                        }
                    }
                    sep(ui);
                    let zoom = self.zoom;
                    egui::ComboBox::from_id_salt("doc-zoom")
                        .width(64.0)
                        .selected_text(format!("{:.0}%", zoom * 100.0))
                        .show_ui(ui, |ui| {
                            for z in [0.5, 0.75, 0.9, 1.0, 1.25, 1.5, 2.0] {
                                if ui
                                    .selectable_label(
                                        (zoom - z).abs() < 0.01,
                                        format!("{:.0}%", z * 100.0),
                                    )
                                    .clicked()
                                {
                                    cmds.push(Cmd::Zoom(z));
                                }
                            }
                        });
                    sep(ui);
                    let line = format::line_of(&self.buf().source, self.caret);
                    let current = format::line_range(&self.buf().source, line)
                        .map(|r| format::style_of(&self.buf().source[r]))
                        .unwrap_or(Style::Vo);
                    ui.add_enabled_ui(editing, |ui| {
                        egui::ComboBox::from_id_salt("doc-style")
                            .width(118.0)
                            .height(520.0)
                            .selected_text(
                                RichText::new(current.short()).color(style_color(current)),
                            )
                            .show_ui(ui, |ui| {
                                ui.set_min_width(230.0);
                                for s in Style::ALL {
                                    let r = ui.selectable_label(current == s, style_preview(s));
                                    if r.clicked() {
                                        cmds.push(Cmd::Style(s));
                                    }
                                }
                                ui.separator();
                                if ui
                                    .button(format!(
                                        "Update \u{2018}{}\u{2019} to match",
                                        current.short()
                                    ))
                                    .clicked()
                                {
                                    cmds.push(Cmd::UpdateStyle);
                                }
                                if ui
                                    .button(format!("Reset \u{2018}{}\u{2019}", current.short()))
                                    .clicked()
                                {
                                    cmds.push(Cmd::ResetStyle);
                                }
                                if ui.button("Paragraph styles…").clicked() {
                                    cmds.push(Cmd::Styles);
                                }
                            });
                    });
                    sep(ui);
                    let props = {
                        let b = self.buf();
                        let lines = doc::lines(&b.source, &b.parsed);
                        doc::props_at(&b.source, &lines, self.caret)
                    };
                    ui.add_enabled_ui(editing, |ui| {
                        if let Some(f) =
                            font_picker(ui, "doc-font", &props.font, 128.0, &mut self.font_filter)
                        {
                            cmds.push(Cmd::Font(f));
                        }
                    });
                    sep(ui);
                    if icons::button(
                        ui,
                        Icon::Minus,
                        "Decrease font size (Ctrl+Shift+,)",
                        editing,
                        false,
                    )
                    .clicked()
                    {
                        cmds.push(Cmd::SizeStep(-1.0));
                    }
                    {
                        let shown = fmt_num(props.size);
                        let draft = self.size_draft.get_or_insert_with(|| shown.clone());
                        let r = ui.add_enabled(
                            editing,
                            egui::TextEdit::singleline(draft)
                                .desired_width(34.0)
                                .horizontal_align(Align::Center)
                                .font(FontId::proportional(12.5)),
                        );
                        if r.lost_focus() {
                            if let Ok(v) = draft.trim().parse::<f32>()
                                && fmt_num(v) != shown
                            {
                                cmds.push(Cmd::Size(v));
                            }
                            self.size_draft = None;
                        } else if !r.has_focus() {
                            self.size_draft = None;
                        }
                    }
                    if icons::button(
                        ui,
                        Icon::Plus,
                        "Increase font size (Ctrl+Shift+.)",
                        editing,
                        false,
                    )
                    .clicked()
                    {
                        cmds.push(Cmd::SizeStep(1.0));
                    }
                    sep(ui);
                    let b = icons::letter(ui, "B", theme::DOC_BOLD, "Bold (Ctrl+B)", editing, None);
                    let i =
                        icons::letter(ui, "I", theme::DOC_ITALIC, "Italic (Ctrl+I)", editing, None);
                    let u = icons::letter(
                        ui,
                        "U",
                        theme::DOC,
                        "Underline (Ctrl+U)",
                        editing,
                        Some("under"),
                    );
                    let s = icons::letter(
                        ui,
                        "S",
                        theme::DOC,
                        "Strikethrough (Alt+Shift+5)",
                        editing,
                        Some("strike"),
                    );
                    for (r, c) in [
                        (b, Cmd::Bold),
                        (i, Cmd::Italic),
                        (u, Cmd::Underline),
                        (s, Cmd::Strike),
                    ] {
                        if r.clicked() {
                            cmds.push(c);
                        }
                    }
                    // Text color: an "A" with a bar in the current color.
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(28.0, 28.0),
                        if editing {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    );
                    {
                        let p = ui.painter();
                        if resp.hovered() && editing {
                            p.rect_filled(rect, 5.0, theme::BG_HOVER);
                        }
                        p.text(
                            rect.center() - vec2(0.0, 3.0),
                            Align2::CENTER_CENTER,
                            "A",
                            FontId::new(14.0, FontFamily::Name(theme::DOC_SEMIBOLD.into())),
                            if editing {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            },
                        );
                        p.rect_filled(
                            Rect::from_center_size(rect.center() + vec2(0.0, 8.0), vec2(14.0, 3.0)),
                            1.0,
                            props.color,
                        );
                    }
                    let resp = resp.on_hover_text("Text color");
                    egui::Popup::menu(&resp).show(|ui| {
                        if let Some(c) = palette(ui, "Text color", &mut self.hex_draft, false) {
                            cmds.push(Cmd::Color(c));
                            ui.close();
                        }
                    });
                    let hl = icons::button(
                        ui,
                        Icon::Highlight,
                        "Highlight color",
                        editing,
                        props.highlight.is_some(),
                    );
                    egui::Popup::menu(&hl).show(|ui| {
                        if let Some(c) = palette(ui, "Highlight", &mut self.hex_draft, true) {
                            cmds.push(Cmd::Highlight(c));
                            ui.close();
                        }
                    });
                    sep(ui);
                    for (ic, c, tip, en) in [
                        (Icon::Link, Cmd::Link, "Insert link (Ctrl+K)", editing),
                        (
                            Icon::Comment,
                            Cmd::Comment,
                            "Add comment (Ctrl+Alt+M)",
                            true,
                        ),
                        (
                            Icon::Clip,
                            Cmd::LinkClip,
                            "Link a clip to the selection (opens Media)",
                            self.buf().main,
                        ),
                    ] {
                        if icons::button(ui, ic, tip, en, false).clicked() {
                            cmds.push(c);
                        }
                    }
                    sep(ui);
                    let align_icon = match props.align {
                        doc::HAlign::Left => Icon::AlignLeft,
                        doc::HAlign::Center => Icon::AlignCenter,
                        doc::HAlign::Right => Icon::AlignRight,
                    };
                    let ar = icons::button(ui, align_icon, "Align", editing, false);
                    egui::Popup::menu(&ar).show(|ui| {
                        ui.horizontal(|ui| {
                            for (a, ic, tip) in [
                                (doc::HAlign::Left, Icon::AlignLeft, "Left (Ctrl+Shift+L)"),
                                (
                                    doc::HAlign::Center,
                                    Icon::AlignCenter,
                                    "Center (Ctrl+Shift+E)",
                                ),
                                (doc::HAlign::Right, Icon::AlignRight, "Right (Ctrl+Shift+R)"),
                            ] {
                                if icons::button(ui, ic, tip, true, props.align == a).clicked() {
                                    cmds.push(Cmd::Align(a));
                                    ui.close();
                                }
                            }
                        });
                    });
                    let sr = icons::button(ui, Icon::Spacing, "Line spacing", editing, false);
                    egui::Popup::menu(&sr).show(|ui| {
                        for (v, name) in [
                            (1.0, "Single"),
                            (1.15, "1.15"),
                            (1.2, "1.2 (default)"),
                            (1.5, "1.5"),
                            (2.0, "Double"),
                        ] {
                            if ui
                                .selectable_label((props.spacing - v).abs() < 0.01, name)
                                .clicked()
                            {
                                cmds.push(Cmd::Spacing(v));
                                ui.close();
                            }
                        }
                    });
                    for (ic, c, tip) in [
                        (
                            Icon::Checklist,
                            Cmd::List(List::Check),
                            "Checklist (Ctrl+Shift+9)",
                        ),
                        (
                            Icon::Bullets,
                            Cmd::List(List::Bullet),
                            "Bulleted list (Ctrl+Shift+8)",
                        ),
                        (
                            Icon::Numbered,
                            Cmd::List(List::Numbered),
                            "Numbered list (Ctrl+Shift+7)",
                        ),
                        (Icon::Outdent, Cmd::Outdent, "Decrease indent (Ctrl+[)"),
                        (Icon::Indent, Cmd::Indent, "Increase indent (Ctrl+])"),
                        (Icon::Clear, Cmd::Clear, "Clear formatting (Ctrl+\\)"),
                    ] {
                        if icons::button(ui, ic, tip, editing, false).clicked() {
                            cmds.push(c);
                        }
                    }
                    sep(ui);
                    let compact = ui.available_width() < 430.0;
                    for k in [CueKind::Sfx, CueKind::Music, CueKind::Look, CueKind::Clip] {
                        if cue_button(
                            ui,
                            k.icon(),
                            if compact { "" } else { k.label() },
                            widgets::cue_color(k.name()),
                            editing,
                        )
                        .clicked()
                        {
                            cmds.push(Cmd::Cue(k));
                        }
                    }
                    if cue_button(
                        ui,
                        Icon::Transition,
                        if compact { "" } else { "Divider" },
                        theme::CUE_TRANSITION,
                        editing,
                    )
                    .clicked()
                    {
                        cmds.push(Cmd::Divider);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(text("Type / for cues", 11.5, theme::TEXT_MUTED));
                    });
                });
            });
    }

    fn menu_search(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let r = Frame::new()
            .fill(theme::BG_INSET)
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    icons::paint(ui.painter(), ir, Icon::Search, theme::TEXT_2);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.menu_search)
                            .id(Id::new("doc-menu-search"))
                            .hint_text("Menus")
                            .frame(Frame::NONE)
                            .desired_width(78.0),
                    )
                })
                .inner
            })
            .inner;
        let q = self.menu_search.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.menu_search.clear();
            return;
        }
        let hits: Vec<Cmd> = all_cmds()
            .into_iter()
            .filter(|c| c.label().to_lowercase().contains(&q))
            .take(9)
            .collect();
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        egui::Area::new(Id::new("doc-menu-search-results"))
            .order(egui::Order::Foreground)
            .fixed_pos(r.rect.left_bottom() + vec2(-8.0, 8.0))
            .show(ui.ctx(), |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(260.0);
                    if hits.is_empty() {
                        ui.label(muted("No matching commands"));
                    }
                    for (i, c) in hits.iter().enumerate() {
                        let b = ui.add(
                            egui::Button::selectable(i == 0, c.label()).shortcut_text(c.shortcut()),
                        );
                        if b.clicked() || (i == 0 && enter) {
                            cmds.push(c.clone());
                            self.menu_search.clear();
                        }
                    }
                });
            });
    }

    // --------------------------------------------------------------- panels

    fn tabs_panel(&mut self, ui: &mut egui::Ui, project: &Project) {
        let mut open: Option<String> = None;
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Left, "Hide tabs", true, false).clicked() {
                self.show_tabs = false;
            }
            ui.label(strong("Document tabs", 13.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, Icon::Plus, "New tab", true, false).clicked() {
                    self.new_tab = Some(String::new());
                }
            });
        });
        ui.add_space(6.0);
        if let Some(name) = &mut self.new_tab {
            let r =
                ui.add(egui::TextEdit::singleline(name).hint_text("Tab name, e.g. Series bible"));
            r.request_focus();
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                match project.new_tab(name) {
                    Ok(rel) => {
                        self.tabs = project.tabs();
                        open = Some(rel);
                    }
                    Err(e) => self.buf_mut().error = Some(e.to_string()),
                }
                self.new_tab = None;
            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.new_tab = None;
            }
            ui.add_space(4.0);
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for t in self.tabs.clone() {
                    let active = t.rel == self.buf().rel;
                    let label = if t.main {
                        self.main
                            .parsed
                            .front_matter
                            .as_ref()
                            .and_then(|f| f.title.clone())
                            .unwrap_or(t.name.clone())
                    } else {
                        t.name.clone()
                    };
                    let r = Frame::new()
                        .fill(if active {
                            theme::tint(theme::ACCENT, 0.15)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (ir, _) =
                                    ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                                icons::paint(
                                    ui.painter(),
                                    ir,
                                    Icon::Doc,
                                    if active { theme::ACCENT } else { theme::TEXT_2 },
                                );
                                let rt = RichText::new(label).size(13.0).color(if active {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_2
                                });
                                let w = ui.available_width() - if t.main { 40.0 } else { 0.0 };
                                ui.allocate_ui(vec2(w.max(20.0), 18.0), |ui| {
                                    ui.add(
                                        egui::Label::new(if active { rt.strong() } else { rt })
                                            .truncate(),
                                    );
                                });
                                if t.main {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(mono(
                                            widgets::mmss(self.main.parsed.est_seconds()),
                                            theme::TEXT_MUTED,
                                        ));
                                    });
                                }
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if r.clicked() && !active {
                        open = Some(t.rel.clone());
                    }
                    if active {
                        self.outline(ui);
                    }
                }
            });
        if let Some(rel) = open {
            self.open_tab(project, &rel);
        }
    }

    fn open_tab(&mut self, project: &Project, rel: &str) {
        if self.buf().dirty {
            self.buf_mut().save(project);
        }
        self.side = if rel == self.main.rel {
            None
        } else {
            Some(Buf::load(project, rel, false))
        };
        self.sel = 0..0;
        self.caret = 0;
        self.slash = None;
        self.composer = None;
    }

    /// Summary and heading outline of the open tab.
    fn outline(&mut self, ui: &mut egui::Ui) {
        let b = self.buf();
        let summary = b.parsed.front_matter.as_ref().and_then(|f| {
            f.raw.lines().find_map(|l| {
                l.strip_prefix("summary:")
                    .map(|v| v.trim().trim_matches('"').to_owned())
            })
        });
        let timeline = b.main.then(|| spearfia_script::timing::timeline(&b.parsed));
        let mut go: Option<usize> = None;
        Frame::new()
            .inner_margin(Margin {
                left: 28,
                right: 4,
                top: 6,
                bottom: 10,
            })
            .show(ui, |ui| {
                ui.label(mono("SUMMARY", theme::TEXT_MUTED));
                match &summary {
                    Some(s) if !s.is_empty() => {
                        ui.label(text(s, 12.0, theme::TEXT_2));
                    }
                    _ => {
                        ui.label(text(
                            "Add summary: … to the front matter.",
                            12.0,
                            theme::TEXT_MUTED,
                        ));
                    }
                }
                ui.add_space(6.0);
                ui.label(mono("OUTLINE", theme::TEXT_MUTED));
                let caret_line = format::line_of(&b.source, self.caret);
                let mut current: Option<usize> = None;
                for (i, blk) in b.parsed.blocks.iter().enumerate() {
                    if let Block::Heading { line, .. } = blk
                        && *line <= caret_line
                    {
                        current = Some(i);
                    }
                }
                for (i, blk) in b.parsed.blocks.iter().enumerate() {
                    let Block::Heading {
                        level,
                        text: t,
                        line,
                        ..
                    } = blk
                    else {
                        continue;
                    };
                    let on = current == Some(i);
                    let time = timeline
                        .as_ref()
                        .and_then(|tl| tl.sections.iter().find(|s| s.block == i))
                        .map(|s| widgets::mmss(s.span.start));
                    let r = ui
                        .horizontal(|ui| {
                            ui.add_space(((*level as f32) - 1.0).max(0.0) * 10.0);
                            if on {
                                let (br, _) =
                                    ui.allocate_exact_size(vec2(2.0, 16.0), Sense::hover());
                                ui.painter().rect_filled(br, 1.0, theme::ACCENT);
                            }
                            let rt = RichText::new(t).size(12.5).color(if on {
                                theme::ACCENT
                            } else {
                                theme::TEXT_2
                            });
                            let w = ui.available_width() - if time.is_some() { 40.0 } else { 0.0 };
                            ui.allocate_ui(vec2(w.max(20.0), 18.0), |ui| {
                                ui.add(
                                    egui::Label::new(if on { rt.strong() } else { rt }).truncate(),
                                );
                            });
                            if let Some(tm) = &time {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(mono(tm, theme::TEXT_MUTED));
                                });
                            }
                        })
                        .response
                        .interact(Sense::click());
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        go = Some(*line);
                    }
                }
            });
        if let Some(line) = go {
            self.scroll_to = Some(line);
            if let Some(r) = format::line_range(&self.buf().source, line) {
                self.pending_sel = Some(r.end..r.end);
            }
        }
    }

    fn status(&mut self, ui: &mut egui::Ui, project: &Project, cmds: &mut Vec<Cmd>) {
        let b = self.buf();
        let words: usize = b
            .parsed
            .blocks
            .iter()
            .map(|blk| match blk {
                Block::Line { segments, .. } => spearfia_script::word_count(segments),
                Block::Heading { text, .. } => text.split_whitespace().count(),
                Block::Divider { .. } => 0,
            })
            .sum();
        let spoken: usize = b.parsed.outline().iter().map(|s| s.spoken_words).sum();
        let line = format::line_of(&b.source, self.caret);
        let line_start = format::line_range(&b.source, line).map_or(0, |r| r.start);
        let col = b.source[line_start..self.caret.min(b.source.len()).max(line_start)]
            .chars()
            .count();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            if self.view == View::Paged {
                ui.label(text(
                    format!("Page {} of {}", self.page_of_caret, self.pages.max(1)),
                    11.5,
                    theme::TEXT_2,
                ));
            }
            ui.label(text(
                format!("{words} words · {spoken} spoken"),
                11.5,
                theme::TEXT_2,
            ));
            if b.main {
                let target = project
                    .file
                    .target_runtime_seconds
                    .map(|t| format!(" · target {}", widgets::mmss(t as f32)))
                    .unwrap_or_default();
                ui.label(text(
                    format!(
                        "~{} at 150 wpm{target}",
                        widgets::mmss(b.parsed.est_seconds())
                    ),
                    11.5,
                    theme::TEXT_2,
                ));
            }
            ui.label(mono(
                format!("Ln {}, Col {}", line + 1, col + 1),
                theme::TEXT_MUTED,
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for v in [View::Markdown, View::Pageless, View::Paged] {
                    let name = match v {
                        View::Paged => "Paged",
                        View::Pageless => "Pageless",
                        View::Markdown => "Markdown",
                    };
                    let r = ui.add(
                        egui::Label::new(text(
                            name,
                            11.5,
                            if self.view == v {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            },
                        ))
                        .sense(Sense::click()),
                    );
                    if r.clicked() {
                        cmds.push(Cmd::View(v));
                    }
                }
            });
        });
    }
}

// The canvas lives in its own impl block for readability.
impl ScriptStage {
    fn canvas(&mut self, ui: &mut egui::Ui, project: &Project, links: &Links, cmds: &mut Vec<Cmd>) {
        let side = if self.show_comments { 340.0 } else { 0.0 };
        let paged = self.view == View::Paged;
        let z = self.zoom;
        // Ruler, fixed above the scrolling page.
        let full = ui.available_rect_before_wrap();
        let page_x = |avail: Rect| -> (f32, f32) {
            if paged {
                let r = script_view::page_rect(avail, z, side, 10.0);
                (r.left(), r.width())
            } else {
                let w = (avail.width() - side - 96.0).clamp(420.0, 980.0 * z);
                (
                    avail.left() + ((avail.width() - side - w) / 2.0).max(24.0),
                    w,
                )
            }
        };
        if paged {
            let (rr, _) = ui.allocate_exact_size(vec2(full.width(), 22.0), Sense::hover());
            let (px, pw) = page_x(full);
            ruler(ui.painter(), rr, px, pw, z);
        }
        egui::ScrollArea::vertical()
            .id_salt(("doc-scroll", self.buf().rel.clone()))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let avail = ui.available_rect_before_wrap();
                let (px, pw) = page_x(avail);
                let (origin, text_w) = if paged {
                    (
                        pos2(
                            px + doc::MARGIN_L * z,
                            avail.top() + 28.0 + doc::MARGIN_T * z,
                        ),
                        doc::text_width(z),
                    )
                } else {
                    (pos2(px + 48.0, avail.top() + 36.0), pw - 96.0)
                };
                self.editor(ui, project, links, origin, text_w, (px, pw), avail, cmds);
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn editor(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        links: &Links,
        origin: Pos2,
        text_w: f32,
        (px, pw): (f32, f32),
        avail: Rect,
        cmds: &mut Vec<Cmd>,
    ) {
        let ctx = ui.ctx().clone();
        let id = self.editor_id();
        let z = self.zoom;
        let paged = self.view == View::Paged;
        let editing = self.can_edit();
        let now = ui.input(|i| i.time);

        // Apply a pending selection (after a toolbar action, outline click, comment click).
        if let Some(r) = self.pending_sel.take() {
            let idx = doc::Index::new(&self.buf().source);
            let mut st = TextEditState::load(&ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(idx.char(r.start)),
                CCursor::new(idx.char(r.end)),
            )));
            st.store(&ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let caret_line = TextEditState::load(&ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let idx = doc::Index::new(&self.buf().source);
                format::line_of(&self.buf().source, idx.byte(r.primary.index.0))
            });

        // Slash menu keys, taken before the editor sees them.
        let mut slash_pick: Option<usize> = None;
        if let Some(s) = &mut self.slash {
            let n = filtered_items(&s.query).len();
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    s.selected = (s.selected + 1) % n.max(1);
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    s.selected = (s.selected + n.max(1) - 1) % n.max(1);
                }
                if i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::NONE, Key::Tab)
                {
                    slash_pick = Some(s.selected);
                }
            });
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.slash = None;
            }
        }

        // Marks: links, comments, find results.
        let b = self.buf();
        let mut marks: Vec<Mark> = Vec::new();
        let resolved = b.main.then(|| script_view::resolve(&b.parsed, links));
        if let Some(r) = &resolved {
            marks.extend(script_view::link_marks(&b.source, &b.parsed, r, None));
        }
        for c in self.comments.open_on(&b.rel) {
            if let Some(r) = comments::find_quote(&b.source, &c.quote, c.line) {
                let on = self.active_comment.as_deref() == Some(c.id.as_str());
                let color = if c.suggestion.is_some() {
                    theme::OK
                } else {
                    theme::WARN
                };
                marks.push(Mark {
                    range: r,
                    bg: theme::tint(color, if on { 0.32 } else { 0.14 }),
                    underline: Some(theme::tint(color, 0.7)),
                    color: None,
                });
            }
        }
        if let Some(c) = &self.composer {
            marks.push(Mark::bg(c.range.clone(), theme::tint(theme::ACCENT, 0.3)));
        }
        if let Some((q, _, _)) = &self.find
            && !q.is_empty()
        {
            for (i, _) in b.source.match_indices(q.as_str()) {
                marks.push(Mark::bg(
                    i..i + q.len(),
                    theme::tint(theme::CUE_TRANSITION, 0.3),
                ));
            }
        }
        let look = Look {
            zoom: z,
            reveal: if editing && focused { caret_line } else { None },
            width: text_w,
        };

        // Page background slot (painted once the height is known).
        let bg = ui.painter().add(egui::Shape::Noop);
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
            let s = buf.as_str();
            let parsed = spearfia_script::parse(s);
            let lines = doc::lines(s, &parsed);
            ui.painter()
                .layout_job(doc::job(ui.painter(), s, &lines, &marks, look))
        };
        let text_rect = Rect::from_min_size(origin, vec2(text_w, 100.0));
        let mut child = ui.new_child(
            UiBuilder::new().max_rect(Rect::from_min_size(origin, vec2(text_w, f32::INFINITY))),
        );
        let out = if editing {
            let b = self.side.as_mut().unwrap_or(&mut self.main);
            let out = egui::TextEdit::multiline(&mut b.source)
                .id(id)
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .desired_width(text_w)
                .desired_rows(1)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(&mut child);
            if out.response.response.changed() {
                b.changed(now);
            }
            out
        } else {
            let copy = self.buf().source.clone();
            let mut ro: &str = &copy;
            egui::TextEdit::multiline(&mut ro)
                .id(id)
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .desired_width(text_w)
                .desired_rows(1)
                .layouter(&mut layouter)
                .show(&mut child)
        };
        let _ = text_rect;
        let changed = out.response.response.changed();
        let galley = out.galley.clone();
        let gpos = out.galley_pos;

        // Selection and caret, in bytes.
        let src = self.buf().source.clone();
        let idx = doc::Index::new(&src);
        if let Some(cr) = out.cursor_range {
            let [a, bb] = cr.sorted_cursors();
            self.sel = idx.byte(a.index.0)..idx.byte(bb.index.0);
            self.caret = idx.byte(cr.primary.index.0);
        }
        self.sel.end = self.sel.end.min(src.len());
        self.sel.start = self.sel.start.min(self.sel.end);
        self.caret = self.caret.min(src.len());

        // Page sheet behind the text.
        let text_h = galley.size().y.max(20.0);
        let page = if paged {
            Rect::from_min_size(
                pos2(px, avail.top() + 28.0),
                vec2(pw, text_h + (doc::MARGIN_T + doc::MARGIN_B) * z),
            )
        } else {
            Rect::from_min_size(pos2(px, avail.top() + 12.0), vec2(pw, text_h + 72.0))
        };
        if paged {
            ui.painter().set(
                bg,
                egui::Shape::Vec(vec![
                    egui::Shape::rect_filled(
                        page.translate(vec2(0.0, 3.0)).expand(1.0),
                        3.0,
                        Color32::from_black_alpha(90),
                    ),
                    egui::Shape::rect_filled(page, 2.0, theme::BG_RAISED),
                    egui::Shape::rect_stroke(
                        page,
                        2.0,
                        Stroke::new(1.0, theme::BORDER),
                        egui::StrokeKind::Inside,
                    ),
                ]),
            );
            // Page breaks.
            let ph = doc::PAGE_H * z;
            self.pages = ((page.height() / ph).ceil() as usize).max(1);
            let caret_y = doc::pos_of(&galley, gpos, &idx, self.caret).top() - page.top();
            self.page_of_caret = ((caret_y / ph).floor() as usize + 1).min(self.pages);
            for k in 1..self.pages {
                let y = page.top() + k as f32 * ph;
                let p = ui.painter();
                p.extend(egui::Shape::dashed_line(
                    &[pos2(page.left(), y), pos2(page.right(), y)],
                    Stroke::new(1.0, theme::BORDER_STRONG),
                    6.0,
                    5.0,
                ));
                p.text(
                    pos2(page.right() - 10.0, y + 4.0),
                    Align2::RIGHT_TOP,
                    format!("Page {}", k + 1),
                    FontId::proportional(10.5),
                    theme::TEXT_MUTED,
                );
            }
        }
        ui.allocate_rect(
            Rect::from_min_size(
                avail.min,
                vec2(avail.width(), page.bottom() - avail.top() + 80.0),
            ),
            Sense::hover(),
        );

        // Margin: VO/LIVE tags, coverage, bullets, checkboxes, section times.
        let parsed = spearfia_script::parse(&src);
        let lines = doc::lines(&src, &parsed);
        let covered = resolved.as_ref().map(|r| script_view::coverage(&parsed, r));
        let times: Option<HashMap<usize, String>> = self.buf().main.then(|| {
            let tl = spearfia_script::timing::timeline(&parsed);
            tl.sections
                .iter()
                .filter_map(|s| match parsed.blocks.get(s.block) {
                    Some(Block::Heading { line, .. }) => Some((
                        *line,
                        format!(
                            "{}–{}",
                            widgets::mmss(s.span.start),
                            widgets::mmss(s.span.end)
                        ),
                    )),
                    _ => None,
                })
                .collect()
        });
        let checks = doc::paint_margin(
            ui.painter(),
            &galley,
            gpos,
            &src,
            &lines,
            look,
            &doc::Margin {
                covered: covered.as_ref(),
                times: times.as_ref(),
            },
        );
        for (line, r) in checks {
            let resp = ui.interact(r, Id::new(("doc-check", line)), Sense::click());
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && editing {
                let b = self.buf_mut();
                b.source = format::toggle_check(&b.source, line);
                b.changed(now);
            }
        }

        // Scroll to a line (outline click).
        if let Some(line) = self.scroll_to.take()
            && let Some(l) = lines.get(line)
        {
            ui.scroll_to_rect(
                doc::rows_rect(&galley, gpos, &idx, &l.range).expand(40.0),
                Some(Align::TOP),
            );
        }

        // Slash menu: opened by typing `/`, kept while the word after it is typed.
        if editing && changed {
            self.slash = detect_slash(&src, self.caret, self.slash.is_some());
        } else if self.slash.as_ref().is_some_and(|s| {
            self.caret <= s.start
                || format::line_of(&src, self.caret) != format::line_of(&src, s.start)
        }) {
            self.slash = None;
        }
        if let Some(i) = slash_pick {
            self.apply_slash(i, now);
        } else if let Some(s) = &self.slash {
            let at = doc::pos_of(&galley, gpos, &idx, s.start);
            if let Some(i) = slash_menu(&ctx, at, s) {
                self.apply_slash(i, now);
            }
        }

        // Right margin: selection actions, composer, comment cards.
        let col_x = page.right() + 52.0;
        if !self.sel.is_empty() && self.composer.is_none() {
            let top = doc::pos_of(&galley, gpos, &idx, self.sel.start).top();
            self.margin_actions(ui, pos2(page.right() + 8.0, top), cmds);
        }
        if self.show_comments || self.composer.is_some() {
            self.comment_column(ui, project, &galley, gpos, &idx, col_x, now);
        }
    }

    fn margin_actions(&mut self, ui: &mut egui::Ui, at: Pos2, cmds: &mut Vec<Cmd>) {
        let main = self.buf().main;
        let rect = Rect::from_min_size(at, vec2(36.0, if main { 104.0 } else { 70.0 }));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        Frame::new()
            .fill(theme::BG_RAISED)
            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(4))
            .show(&mut child, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if icons::button(ui, Icon::Comment, "Add comment", true, false).clicked() {
                        cmds.push(Cmd::Comment);
                    }
                    if icons::button(ui, Icon::Pencil, "Suggest an edit", true, false).clicked() {
                        cmds.push(Cmd::Suggest);
                    }
                    if main
                        && icons::button(ui, Icon::Clip, "Link a clip (opens Media)", true, false)
                            .clicked()
                    {
                        cmds.push(Cmd::LinkClip);
                    }
                });
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn comment_column(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        galley: &egui::Galley,
        gpos: Pos2,
        idx: &doc::Index,
        x: f32,
        _now: f64,
    ) {
        let src = self.buf().source.clone();
        let rel = self.buf().rel.clone();
        let w = 300.0;
        let mut y_min = f32::MIN;
        let author = self.author.clone();
        let stamp = comments::now();

        // The composer (new comment / suggestion) sits level with the selection.
        if let Some(c) = &mut self.composer {
            let top = doc::pos_of(galley, gpos, idx, c.range.start).top();
            let mut done: Option<bool> = None;
            let r = card(ui, pos2(x, top), w, true, |ui| {
                ui.horizontal(|ui| {
                    avatar(ui, &author, theme::CUE_VO);
                    ui.label(strong(&author, 12.5));
                });
                if c.kind == ComposerKind::Suggest {
                    ui.label(text(
                        format!("Replace \u{201C}{}\u{201D} with", short(&c.quote, 60)),
                        12.0,
                        theme::TEXT_2,
                    ));
                }
                let te = ui.add(
                    egui::TextEdit::multiline(&mut c.text)
                        .hint_text(if c.kind == ComposerKind::Suggest {
                            "New text"
                        } else {
                            "Comment"
                        })
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if te.gained_focus() || c.text.is_empty() && !te.has_focus() {
                    te.request_focus();
                }
                ui.horizontal(|ui| {
                    let label = if c.kind == ComposerKind::Suggest {
                        "Suggest"
                    } else {
                        "Comment"
                    };
                    if widgets::primary_button(ui, label).clicked()
                        || (ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter)))
                    {
                        done = Some(true);
                    }
                    if widgets::button(ui, "Cancel").clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        done = Some(false);
                    }
                });
            });
            y_min = r.bottom() + 10.0;
            match done {
                Some(true) => {
                    let c = self.composer.take().expect("composer");
                    let suggestion = (c.kind == ComposerKind::Suggest).then(|| c.text.clone());
                    let body = if c.kind == ComposerKind::Suggest {
                        String::new()
                    } else {
                        c.text.trim().to_owned()
                    };
                    if !body.is_empty() || suggestion.is_some() {
                        let id = self.comments.add(Comment {
                            id: String::new(),
                            doc: rel.clone(),
                            quote: c.quote,
                            line: c.line,
                            author: author.clone(),
                            body,
                            created: stamp,
                            suggestion,
                            resolved: false,
                            replies: vec![],
                        });
                        self.active_comment = Some(id);
                        self.save_comments(project);
                    }
                    self.show_comments = true;
                }
                Some(false) => self.composer = None,
                None => {}
            }
        }
        if !self.show_comments {
            return;
        }

        // Cards, level with their text, pushed down so they don't overlap.
        let mut placed: Vec<(f32, Comment, Option<Range<usize>>)> = self
            .comments
            .open_on(&rel)
            .map(|c| {
                let r = comments::find_quote(&src, &c.quote, c.line);
                let y = r
                    .as_ref()
                    .map_or(f32::MAX, |r| doc::pos_of(galley, gpos, idx, r.start).top());
                (y, c.clone(), r)
            })
            .collect();
        placed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut action: Option<(String, &'static str)> = None;
        let mut select: Option<Range<usize>> = None;
        let bottom_y = galley.size().y + gpos.y;
        for (y, c, range) in placed {
            let y = if y == f32::MAX {
                bottom_y.max(y_min)
            } else {
                y.max(y_min)
            };
            let on = self.active_comment.as_deref() == Some(c.id.as_str());
            // Click area first, so the card's own buttons stay on top of it.
            let h = self.card_h.get(&c.id).copied().unwrap_or(0.0);
            let clicked = h > 0.0
                && ui
                    .interact(
                        Rect::from_min_size(pos2(x, y), vec2(w, h)),
                        Id::new(("comment-card", &c.id)),
                        Sense::click(),
                    )
                    .clicked();
            let r = card(ui, pos2(x, y), w, on, |ui| {
                ui.horizontal(|ui| {
                    avatar(
                        ui,
                        &c.author,
                        if c.suggestion.is_some() {
                            theme::OK
                        } else {
                            theme::ACCENT
                        },
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(strong(&c.author, 12.5));
                        ui.label(mono(
                            if c.suggestion.is_some() {
                                format!("suggested · {}", widgets::ago(c.created, stamp))
                            } else {
                                widgets::ago(c.created, stamp)
                            },
                            theme::TEXT_MUTED,
                        ));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if icons::button(ui, Icon::Close, "Delete", true, false).clicked() {
                            action = Some((c.id.clone(), "delete"));
                        }
                        if c.suggestion.is_none()
                            && icons::button(ui, Icon::Check, "Resolve", true, false).clicked()
                        {
                            action = Some((c.id.clone(), "resolve"));
                        }
                    });
                });
                if range.is_none() {
                    ui.label(text(
                        "The commented text is no longer in the script.",
                        11.5,
                        theme::WARN,
                    ));
                }
                if let Some(s) = &c.suggestion {
                    let mut job = egui::text::LayoutJob::default();
                    let f = |c: Color32, strike: bool| egui::TextFormat {
                        font_id: FontId::proportional(12.5),
                        color: c,
                        strikethrough: if strike {
                            Stroke::new(1.0, c)
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    };
                    job.append("Replace: ", 0.0, f(theme::TEXT, false));
                    job.append(&short(&c.quote, 80), 0.0, f(theme::DANGER, true));
                    job.append(" with ", 0.0, f(theme::TEXT, false));
                    job.append(
                        if s.is_empty() { "(nothing)" } else { s },
                        0.0,
                        f(theme::OK, false),
                    );
                    ui.label(job);
                    ui.horizontal(|ui| {
                        if range.is_some() && widgets::primary_button(ui, "Accept").clicked() {
                            action = Some((c.id.clone(), "accept"));
                        }
                        if widgets::button(ui, "Reject").clicked() {
                            action = Some((c.id.clone(), "delete"));
                        }
                    });
                } else {
                    ui.label(text(
                        format!("\u{201C}{}\u{201D}", short(&c.quote, 70)),
                        11.5,
                        theme::TEXT_MUTED,
                    ));
                }
                if !c.body.is_empty() {
                    ui.label(text(&c.body, 12.5, theme::TEXT));
                }
                for rp in &c.replies {
                    widgets::rule(ui);
                    ui.horizontal(|ui| {
                        avatar(ui, &rp.author, theme::CUE_VO);
                        ui.label(strong(&rp.author, 12.0));
                        ui.label(mono(widgets::ago(rp.created, stamp), theme::TEXT_MUTED));
                    });
                    ui.label(text(&rp.body, 12.5, theme::TEXT));
                }
                if on {
                    let draft = self.reply_drafts.entry(c.id.clone()).or_default();
                    let te = ui.add(
                        egui::TextEdit::singleline(draft)
                            .hint_text("Reply")
                            .desired_width(f32::INFINITY),
                    );
                    if te.lost_focus()
                        && ui.input(|i| i.key_pressed(Key::Enter))
                        && !draft.trim().is_empty()
                    {
                        action = Some((c.id.clone(), "reply"));
                    }
                }
            });
            self.card_h.insert(c.id.clone(), r.height());
            if clicked {
                self.active_comment = Some(c.id.clone());
                if let Some(rr) = &range {
                    select = Some(rr.clone());
                }
            }
            y_min = r.bottom() + 10.0;
        }
        if let Some(r) = select {
            self.pending_sel = Some(r);
        }
        if let Some((id, what)) = action {
            self.comment_action(project, &id, what);
        }
    }

    fn comment_action(&mut self, project: &Project, id: &str, what: &str) {
        match what {
            "delete" => self.comments.remove(id),
            "resolve" => {
                if let Some(c) = self.comments.get_mut(id) {
                    c.resolved = true;
                }
            }
            "reply" => {
                let body = self.reply_drafts.remove(id).unwrap_or_default();
                let author = self.author.clone();
                if let Some(c) = self.comments.get_mut(id) {
                    c.replies.push(Reply {
                        author,
                        body: body.trim().to_owned(),
                        created: comments::now(),
                    });
                }
            }
            "accept" => {
                let Some(c) = self.comments.comments.iter().find(|c| c.id == id).cloned() else {
                    return;
                };
                let b = self.buf_mut();
                // The suggestion belongs to one document; only apply it there.
                let applied = (b.rel == c.doc)
                    .then(|| comments::apply_suggestion(&b.source, &c))
                    .flatten();
                let Some(out) = applied else {
                    self.comments_error = Some(format!(
                        "The suggestion wasn't applied: \"{}\" is no longer in {}. Edit the text by hand, or delete the suggestion.",
                        short(&c.quote, 60),
                        c.doc
                    ));
                    return;
                };
                b.source = out;
                b.changed(0.0);
                b.save(project);
                if let Some(c) = self.comments.get_mut(id) {
                    c.resolved = true;
                }
            }
            _ => {}
        }
        self.save_comments(project);
    }

    fn save_comments(&mut self, project: &Project) {
        if let Err(e) = self.comments.save(project) {
            self.comments_error = Some(e.to_string());
        }
    }

    fn markdown(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let editing = self.can_edit();
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                Frame::new().inner_margin(Margin::same(20)).show(ui, |ui| {
                    let b = self.side.as_mut().unwrap_or(&mut self.main);
                    if editing {
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut b.source)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(40),
                        );
                        if r.changed() {
                            b.changed(now);
                        }
                    } else {
                        let mut ro: &str = &b.source;
                        ui.add(
                            egui::TextEdit::multiline(&mut ro)
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    }
                });
            });
    }
}

// Commands.
impl ScriptStage {
    fn run(&mut self, ctx: &egui::Context, project: &Project, cmd: Cmd) {
        if cmd.edits() && !self.can_edit() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let id = self.editor_id();
        let sel = self.sel.clone();
        let src = self.buf().source.clone();
        let lines = format::lines_of(&src, &sel);
        let caret_line = format::line_of(&src, self.caret);
        let set = |me: &mut Self, s: String, r: Range<usize>| {
            let b = me.buf_mut();
            b.source = s;
            b.changed(now);
            me.pending_sel = Some(r);
        };
        let end_of = |s: &str, line: usize| format::line_range(s, line).map_or(s.len(), |r| r.end);
        match cmd {
            Cmd::Save => {
                for b in std::iter::once(&mut self.main).chain(self.side.as_mut()) {
                    b.save(project);
                }
            }
            Cmd::Reload => {
                let rel = self.buf().rel.clone();
                let main = self.buf().main;
                *self.buf_mut() = Buf::load(project, &rel, main);
                self.tabs = project.tabs();
            }
            Cmd::NewTab => {
                self.show_tabs = true;
                self.new_tab = Some(String::new());
            }
            Cmd::OpenFolder => {
                let dir = project.resolve(&self.buf().rel);
                if let Some(d) = dir.parent() {
                    crate::reveal(d);
                }
            }
            Cmd::Undo | Cmd::Redo => {
                let Some(mut st) = TextEditState::load(ctx, id) else {
                    return;
                };
                let mut u = st.undoer();
                let cur = (st.cursor.char_range().unwrap_or_default(), src.clone());
                let prev = if cmd == Cmd::Undo {
                    u.undo(&cur).cloned()
                } else {
                    u.redo(&cur).cloned()
                };
                if let Some((range, text)) = prev {
                    st.cursor.set_char_range(Some(range));
                    let b = self.buf_mut();
                    b.source = text;
                    b.changed(now);
                }
                st.set_undoer(u);
                st.store(ctx, id);
                ctx.memory_mut(|m| m.request_focus(id));
            }
            Cmd::Find => self.find = Some((src[sel.clone()].to_owned(), String::new(), 0)),
            Cmd::SelectAll => self.pending_sel = Some(0..src.len()),
            Cmd::Bold | Cmd::Italic | Cmd::Underline | Cmd::Strike => {
                let m = match cmd {
                    Cmd::Bold => "**",
                    Cmd::Italic => "*",
                    Cmd::Underline => "++",
                    _ => "~~",
                };
                let (s, r) = format::toggle_wrap(&src, sel, m);
                set(self, s, r);
            }
            Cmd::Link => {
                let r = if sel.is_empty() {
                    format::word_at(&src, sel.start)
                } else {
                    sel
                };
                self.link_prompt = Some((r, "https://".into()));
            }
            Cmd::Comment | Cmd::Suggest => {
                let r = if sel.is_empty() {
                    format::line_range(&src, caret_line).unwrap_or(0..0)
                } else {
                    sel
                };
                if r.is_empty() {
                    return;
                }
                self.composer = Some(Composer {
                    kind: if cmd == Cmd::Suggest {
                        ComposerKind::Suggest
                    } else {
                        ComposerKind::Comment
                    },
                    quote: src[r.clone()].to_owned(),
                    line: format::line_of(&src, r.start),
                    text: if cmd == Cmd::Suggest {
                        src[r.clone()].to_owned()
                    } else {
                        String::new()
                    },
                    range: r,
                });
            }
            Cmd::Style(s) => {
                let out = format::set_style(&src, lines.clone(), s);
                let e = end_of(&out, lines.end - 1);
                set(self, out, e..e);
            }
            Cmd::List(l) => {
                let out = format::toggle_list(&src, lines.clone(), l);
                let e = end_of(&out, lines.end - 1);
                set(self, out, e..e);
            }
            Cmd::Indent | Cmd::Outdent => {
                let out =
                    format::indent(&src, lines.clone(), if cmd == Cmd::Indent { 1 } else { -1 });
                let e = end_of(&out, lines.end - 1);
                set(self, out, e..e);
            }
            Cmd::Clear => {
                let r = if sel.is_empty() {
                    format::line_range(&src, caret_line).unwrap_or(0..0)
                } else {
                    sel
                };
                let (s, r) = format::clear_formatting(&src, r);
                set(self, s, r);
            }
            Cmd::Cue(k) => {
                let (s, r) = insert_cue(
                    &src,
                    &self.buf().parsed,
                    sel.end,
                    k.name(),
                    k.id_prefix(),
                    "",
                );
                set(self, s, r);
            }
            Cmd::ChapterCard => {
                let (s, r) = insert_cue(
                    &src,
                    &self.buf().parsed,
                    sel.end,
                    "look",
                    "l",
                    "chapter-card",
                );
                set(self, s, r);
            }
            Cmd::Divider => {
                let tid = edit::next_id(&self.buf().parsed, "t");
                let (s, at) = format::insert_line_after(
                    &src,
                    caret_line,
                    &format!(":transition[cut]{{#{tid}}}"),
                );
                let label = at - format!("cut]{{#{tid}}}").len();
                set(self, s, label..label + 3);
            }
            Cmd::Table => {
                let (s, at) = format::insert_line_after(
                    &src,
                    caret_line,
                    "| Column | Column |\n|---|---|\n|  |  |",
                );
                set(self, s, at..at);
            }
            Cmd::View(v) => self.view = v,
            Cmd::Zoom(z) => self.zoom = z,
            Cmd::ToggleTabs => self.show_tabs = !self.show_tabs,
            Cmd::ToggleComments => self.show_comments = !self.show_comments,
            Cmd::Mode(m) => {
                self.mode = m;
                self.slash = None;
            }
            Cmd::Help => self.help = true,
            Cmd::WordCount => self.word_count = true,
            Cmd::LinkClip => {
                if let Some(s) = self.selection_words() {
                    self.event = Some(ScriptEvent::LinkClip(s));
                }
            }
            Cmd::Font(f) => self.format_text(&src, sel, caret_line, "font", Some(&f), now),
            Cmd::Size(v) => self.format_text(&src, sel, caret_line, "size", Some(&fmt_num(v)), now),
            Cmd::SizeStep(d) => {
                let lines_ = doc::lines(&src, &self.buf().parsed);
                let cur = doc::props_at(&src, &lines_, self.caret).size;
                let v = (cur + d).clamp(4.0, 200.0);
                self.format_text(&src, sel, caret_line, "size", Some(&fmt_num(v)), now);
            }
            Cmd::Color(c) => self.format_text(&src, sel, caret_line, "color", c.as_deref(), now),
            Cmd::Highlight(c) => {
                self.format_text(&src, sel, caret_line, "highlight", c.as_deref(), now)
            }
            Cmd::Align(a) => {
                let out = spearfia_script::attrs::set_para(
                    &src,
                    lines.clone(),
                    "align",
                    (a != doc::HAlign::Left
                        || self.style_align(&src, caret_line) != doc::HAlign::Left)
                        .then_some(a.key()),
                );
                let keep = self.sel.clone();
                set(self, out, keep);
            }
            Cmd::Spacing(v) => {
                let out = spearfia_script::attrs::set_para(
                    &src,
                    lines.clone(),
                    "spacing",
                    Some(&fmt_num(v)),
                );
                let keep = self.sel.clone();
                set(self, out, keep);
            }
            Cmd::UpdateStyle | Cmd::ResetStyle => {
                let dl = doc::lines(&src, &self.buf().parsed);
                let Some(key) = dl.get(caret_line).and_then(|l| doc::style_key(l.style)) else {
                    return;
                };
                let out = if cmd == Cmd::UpdateStyle {
                    let (stripped, taken) = spearfia_script::attrs::take_para(&src, caret_line);
                    let mut merged = doc::sheet(&src)
                        .into_iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, a)| a)
                        .unwrap_or_default();
                    for (k, v) in taken {
                        spearfia_script::attrs::set(&mut merged, &k, Some(&v));
                    }
                    spearfia_script::attrs::set_style(&stripped, key, Some(&merged))
                } else {
                    spearfia_script::attrs::set_style(&src, key, None)
                };
                // Keep the caret on the same line (the front matter may have grown).
                let delta = out.matches('\n').count() as isize - src.matches('\n').count() as isize;
                let line = (caret_line as isize + delta).max(0) as usize;
                let c = format::line_range(&out, line).map_or(0, |r| r.start);
                set(self, out, c..c);
            }
            Cmd::Styles => self.styles_open = true,
            Cmd::NextMissingClip => {
                let parsed = &self.main.parsed;
                let links = Links::load(project).unwrap_or_default();
                let resolved = script_view::resolve(parsed, &links);
                let after = format::line_of(&self.main.source, self.caret);
                let next = parsed.blocks.iter().enumerate().find_map(|(i, b)| match b {
                    Block::Line {
                        kind: spearfia_script::LineKind::OnScreen,
                        line,
                        ..
                    } if *line > after && !resolved.spans.contains_key(&i) => Some(*line),
                    _ => None,
                });
                if let Some(line) = next {
                    self.scroll_to = Some(line);
                    if let Some(r) = format::line_range(&self.main.source, line) {
                        self.pending_sel = Some(r);
                    }
                }
            }
        }
    }

    /// Apply a text attribute: to the selection as a span, or to the paragraph(s) when
    /// nothing is selected.
    fn format_text(
        &mut self,
        src: &str,
        sel: Range<usize>,
        caret_line: usize,
        key: &str,
        value: Option<&str>,
        now: f64,
    ) {
        use spearfia_script::attrs;
        let (out, range) = if sel.is_empty() {
            let lines = caret_line..caret_line + 1;
            (attrs::set_para(src, lines, key, value), sel)
        } else if format::lines_of(src, &sel).len() > 1 {
            // Several lines: each whole paragraph.
            (
                attrs::set_para(src, format::lines_of(src, &sel), key, value),
                sel,
            )
        } else {
            attrs::set_span(src, sel, key, value)
        };
        let b = self.buf_mut();
        b.source = out;
        b.changed(now);
        self.pending_sel = Some(range);
    }

    fn style_align(&self, src: &str, line: usize) -> doc::HAlign {
        let dl = doc::lines(src, &self.buf().parsed);
        dl.get(line)
            .map(|l| doc::styled(l.style, &doc::sheet(src)).align)
            .unwrap_or(doc::HAlign::Left)
    }

    /// The selection as words of one script line, for linking a clip.
    fn selection_words(&self) -> Option<Selection> {
        let b = &self.main;
        if self.side.is_some() {
            return None;
        }
        let line = format::line_of(&b.source, self.sel.start);
        let block = b
            .parsed
            .blocks
            .iter()
            .position(|blk| matches!(blk, Block::Line { line: l, .. } if *l == line))?;
        let words = format::word_ranges(&b.source, &b.parsed, block);
        if words.is_empty() {
            return None;
        }
        let (s, e) = if self.sel.is_empty() {
            (0, words.len() - 1)
        } else {
            let first = words
                .iter()
                .position(|w| w.end > self.sel.start)
                .unwrap_or(0);
            let last = words
                .iter()
                .rposition(|w| w.start < self.sel.end)
                .unwrap_or(words.len() - 1);
            (first, last.max(first))
        };
        Some(Selection {
            block,
            first: s,
            last: e,
        })
    }

    fn apply_slash(&mut self, i: usize, now: f64) {
        let Some(s) = self.slash.take() else { return };
        let items = filtered_items(&s.query);
        let Some(item) = items.get(i) else { return };
        let action = item.action;
        // Remove the typed `/query`.
        let src = self.buf().source.clone();
        let end = (s.start + 1 + s.query.len()).min(src.len());
        let cleaned = format!("{}{}", &src[..s.start], &src[end..]);
        {
            let b = self.buf_mut();
            b.source = cleaned.clone();
            b.changed(now);
        }
        self.sel = s.start..s.start;
        self.caret = s.start;
        let parsed = spearfia_script::parse(&cleaned);
        let line = format::line_of(&cleaned, s.start);
        let (out, sel) = match action {
            SlashAction::Cmd(k) => {
                insert_cue(&cleaned, &parsed, s.start, k.name(), k.id_prefix(), "")
            }
            SlashAction::Chapter => {
                insert_cue(&cleaned, &parsed, s.start, "look", "l", "chapter-card")
            }
            SlashAction::Divider => {
                let tid = edit::next_id(&parsed, "t");
                let lr = format::line_range(&cleaned, line).unwrap_or(0..0);
                let text = format!(":transition[cut]{{#{tid}}}");
                if cleaned[lr.clone()].trim().is_empty() {
                    let out = format!("{}{text}{}", &cleaned[..lr.start], &cleaned[lr.end..]);
                    (out, lr.start + 12..lr.start + 15)
                } else {
                    let (out, at) = format::insert_line_after(&cleaned, line, &text);
                    let label = at - format!("cut]{{#{tid}}}").len();
                    (out, label..label + 3)
                }
            }
            SlashAction::Style(st) => {
                let out = format::set_style(&cleaned, line..line + 1, st);
                let e = format::line_range(&out, line).map_or(out.len(), |r| r.end);
                (out, e..e)
            }
            SlashAction::List(l) => {
                let out = format::toggle_list(&cleaned, line..line + 1, l);
                let e = format::line_range(&out, line).map_or(out.len(), |r| r.end);
                (out, e..e)
            }
            SlashAction::Table => {
                let (out, at) = format::insert_line_after(
                    &cleaned,
                    line,
                    "| Column | Column |\n|---|---|\n|  |  |",
                );
                (out, at..at)
            }
            SlashAction::Comment => {
                self.sel = format::line_range(&cleaned, line).unwrap_or(0..0);
                self.composer = (!self.sel.is_empty()).then(|| Composer {
                    kind: ComposerKind::Comment,
                    quote: cleaned[self.sel.clone()].to_owned(),
                    line,
                    text: String::new(),
                    range: self.sel.clone(),
                });
                (cleaned.clone(), s.start..s.start)
            }
        };
        let b = self.buf_mut();
        b.source = out;
        b.changed(now);
        self.pending_sel = Some(sel);
    }

    fn windows(&mut self, ctx: &egui::Context, project: &Project, cmds: &mut Vec<Cmd>) {
        let now = ctx.input(|i| i.time);
        // Find and replace.
        let find_count = self.find.as_ref().map_or(0, |(q, _, _)| {
            if q.is_empty() {
                0
            } else {
                self.buf().source.matches(q.as_str()).count()
            }
        });
        let can_edit = self.mode == Mode::Editing;
        if let Some((q, rep, cur)) = &mut self.find {
            let mut open = true;
            let mut action: Option<&str> = None;
            egui::Window::new("Find and replace")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::RIGHT_TOP, vec2(-24.0, 170.0))
                .show(ctx, |ui| {
                    egui::Grid::new("find-grid")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Find");
                            let r = ui.text_edit_singleline(q);
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                action = Some("next");
                            }
                            ui.end_row();
                            ui.label("Replace with");
                            ui.text_edit_singleline(rep);
                            ui.end_row();
                        });
                    let count = find_count;
                    ui.label(muted(format!(
                        "{count} match{}",
                        if count == 1 { "" } else { "es" }
                    )));
                    ui.horizontal(|ui| {
                        if widgets::button(ui, "Next").clicked() {
                            action = Some("next");
                        }
                        ui.add_enabled_ui(can_edit, |ui| {
                            if widgets::button(ui, "Replace").clicked() {
                                action = Some("replace");
                            }
                            if widgets::primary_button(ui, "Replace all").clicked() {
                                action = Some("all");
                            }
                        });
                    });
                });
            let (q, rep) = (q.clone(), rep.clone());
            let _ = cur;
            if !open {
                self.find = None;
            }
            if !q.is_empty() {
                let src = self.buf().source.clone();
                let hits: Vec<usize> = src.match_indices(q.as_str()).map(|(i, _)| i).collect();
                match action {
                    Some("next") if !hits.is_empty() => {
                        let after = self.sel.end;
                        let at = hits
                            .iter()
                            .copied()
                            .find(|&h| h >= after)
                            .unwrap_or(hits[0]);
                        self.pending_sel = Some(at..at + q.len());
                        self.scroll_to = Some(format::line_of(&src, at));
                    }
                    Some("replace") if src.get(self.sel.clone()) == Some(q.as_str()) => {
                        let s = self.sel.clone();
                        let b = self.buf_mut();
                        b.source = format!("{}{rep}{}", &src[..s.start], &src[s.end..]);
                        b.changed(now);
                        self.pending_sel = Some(s.start..s.start + rep.len());
                    }
                    Some("all") => {
                        let b = self.buf_mut();
                        b.source = src.replace(q.as_str(), &rep);
                        b.changed(now);
                    }
                    _ => {}
                }
            }
        }
        // Link URL.
        if let Some((range, url)) = &mut self.link_prompt {
            let mut apply = false;
            let mut close = false;
            egui::Window::new("Insert link")
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_TOP, vec2(0.0, 180.0))
                .show(ctx, |ui| {
                    let src = &self.main.source;
                    let _ = src;
                    let r = ui.add(egui::TextEdit::singleline(url).desired_width(320.0));
                    r.request_focus();
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        apply = true;
                    }
                    ui.horizontal(|ui| {
                        if widgets::primary_button(ui, "Apply").clicked() {
                            apply = true;
                        }
                        if widgets::button(ui, "Cancel").clicked()
                            || ui.input(|i| i.key_pressed(Key::Escape))
                        {
                            close = true;
                        }
                    });
                });
            if apply {
                let (range, url) = (range.clone(), url.clone());
                let (s, r) = format::make_link(&self.buf().source, range, url.trim());
                let b = self.buf_mut();
                b.source = s;
                b.changed(now);
                self.pending_sel = Some(r);
                close = true;
            }
            if close {
                self.link_prompt = None;
            }
        }
        // Paragraph styles.
        if self.styles_open {
            let mut open = true;
            let src = self.buf().source.clone();
            let sheet = doc::sheet(&src);
            let mut change: Option<(String, Option<spearfia_script::attrs::Attrs>)> = None;
            let editing = self.can_edit();
            egui::Window::new("Paragraph styles")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_TOP, vec2(0.0, 150.0))
                .show(ctx, |ui| {
                    ui.label(muted("Stored in this document's front matter (styles:). Every line of that style follows it unless the line overrides it."));
                    ui.add_space(6.0);
                    ui.add_enabled_ui(editing, |ui| {
                        egui::Grid::new("styles-grid").num_columns(8).spacing([10.0, 8.0]).show(ui, |ui| {
                            for h in ["Style", "Font", "Size", "Color", "B", "I", "Align", "Spacing"] {
                                ui.label(mono(h, theme::TEXT_MUTED));
                            }
                            ui.label("");
                            ui.end_row();
                            for (key, ls, name) in [
                                ("title", doc::LineStyle::Title, "Title"),
                                ("h1", doc::LineStyle::H1, "Heading 1"),
                                ("h2", doc::LineStyle::H2, "Heading 2"),
                                ("h3", doc::LineStyle::H3, "Heading 3"),
                                ("vo", doc::LineStyle::Spoken, "VO line"),
                                ("live", doc::LineStyle::Live, "Live line"),
                                ("onscreen", doc::LineStyle::OnScreen, "On-screen"),
                                ("note", doc::LineStyle::Note(doc::NoteKind::Quote), "Note"),
                            ] {
                                let cur = doc::styled(ls, &sheet);
                                let mut a = sheet.iter().find(|(k, _)| k == key).map(|(_, a)| a.clone()).unwrap_or_default();
                                let before = a.clone();
                                let preview_font = crate::fonts::ready(ctx, &cur.font)
                                    .map(|f| crate::fonts::family(&cur.font, if cur.bold && f.bold { crate::fonts::Face::Bold } else { crate::fonts::Face::Regular }))
                                    .unwrap_or(FontFamily::Name(theme::DOC.into()));
                                ui.label(RichText::new(name).font(FontId::new(cur.size.min(20.0), preview_font)).color(cur.color));
                                let mut filter = String::new();
                                if let Some(f) = font_picker(ui, &format!("style-font-{key}"), &cur.font, 120.0, &mut filter) {
                                    spearfia_script::attrs::set(&mut a, "font", Some(&f));
                                }
                                let mut size = cur.size;
                                if ui.add(egui::DragValue::new(&mut size).range(6.0..=96.0).speed(0.25)).changed() {
                                    spearfia_script::attrs::set(&mut a, "size", Some(&fmt_num(size)));
                                }
                                let (cr, cresp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
                                ui.painter().rect(cr, 3.0, cur.color, Stroke::new(1.0, theme::BORDER_STRONG), egui::StrokeKind::Inside);
                                egui::Popup::menu(&cresp).show(|ui| {
                                    let mut hex = String::new();
                                    if let Some(c) = palette(ui, "Color", &mut hex, false) {
                                        spearfia_script::attrs::set(&mut a, "color", c.as_deref());
                                        ui.close();
                                    }
                                });
                                let mut b = cur.bold;
                                if ui.checkbox(&mut b, "").changed() {
                                    spearfia_script::attrs::set(&mut a, "bold", Some(if b { "true" } else { "false" }));
                                }
                                let mut it = cur.italic;
                                if ui.checkbox(&mut it, "").changed() {
                                    spearfia_script::attrs::set(&mut a, "italic", Some(if it { "true" } else { "false" }));
                                }
                                egui::ComboBox::from_id_salt(format!("style-align-{key}"))
                                    .width(70.0)
                                    .selected_text(cur.align.key())
                                    .show_ui(ui, |ui| {
                                        for al in [doc::HAlign::Left, doc::HAlign::Center, doc::HAlign::Right] {
                                            if ui.selectable_label(cur.align == al, al.key()).clicked() {
                                                spearfia_script::attrs::set(&mut a, "align", Some(al.key()));
                                            }
                                        }
                                    });
                                let mut sp = cur.spacing;
                                if ui.add(egui::DragValue::new(&mut sp).range(0.8..=3.0).speed(0.01).fixed_decimals(2)).changed() {
                                    spearfia_script::attrs::set(&mut a, "spacing", Some(&fmt_num(sp)));
                                }
                                if ui.add_enabled(!before.is_empty(), egui::Button::new("Reset")).clicked() {
                                    change = Some((key.to_owned(), None));
                                } else if a != before {
                                    change = Some((key.to_owned(), Some(a)));
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
            if let Some((key, a)) = change {
                let out = spearfia_script::attrs::set_style(&src, &key, a.as_ref());
                let shift = out.len() as isize - src.len() as isize;
                let b = self.buf_mut();
                b.source = out;
                b.changed(now);
                self.caret = (self.caret as isize + shift).max(0) as usize;
            }
            if !open {
                self.styles_open = false;
            }
        }
        // Word count.
        if self.word_count {
            let b = self.buf();
            let words: usize = b
                .parsed
                .blocks
                .iter()
                .map(|blk| match blk {
                    Block::Line { segments, .. } => spearfia_script::word_count(segments),
                    Block::Heading { text, .. } => text.split_whitespace().count(),
                    Block::Divider { .. } => 0,
                })
                .sum();
            let spoken: usize = b.parsed.outline().iter().map(|s| s.spoken_words).sum();
            let chars = b.source.chars().filter(|c| !c.is_whitespace()).count();
            let cues = edit::directives(&b.parsed).count();
            let est = widgets::mmss(b.parsed.est_seconds());
            egui::Window::new("Word count")
                .open(&mut self.word_count)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    egui::Grid::new("wc")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            for (k, v) in [
                                ("Words", words.to_string()),
                                ("Spoken words (VO + live)", spoken.to_string()),
                                ("Characters (no spaces)", chars.to_string()),
                                ("Cues", cues.to_string()),
                                ("Spoken runtime at 150 wpm", est),
                            ] {
                                ui.label(k);
                                ui.label(strong(v, 13.0));
                                ui.end_row();
                            }
                        });
                });
        }
        // Help.
        if self.help {
            egui::Window::new("Script syntax")
                .open(&mut self.help)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    egui::Grid::new("help")
                        .num_columns(2)
                        .spacing([20.0, 6.0])
                        .show(ui, |ui| {
                            for (k, v) in [
                                ("# Title", "Title"),
                                ("## Section", "Heading 1: a section, timed"),
                                ("### / ####", "Headings 2 and 3"),
                                ("A plain line", "VO, spoken"),
                                ("LIVE: …", "Spoken to camera"),
                                ("[ON SCREEN: …]", "What we see"),
                                (
                                    "> …   - …   1. …   - [ ] …   | … |",
                                    "Notes, lists, checklists, tables (not spoken)",
                                ),
                                (
                                    "**bold**  *italic*  ++underline++  ~~strike~~",
                                    "Inline formatting",
                                ),
                                ("[text](https://…)", "Link"),
                                (":sfx[boom]{#s1}", "Cue: sfx, music, look, clip"),
                                (":transition[whip-pan]{#t1}", "On its own line: a divider"),
                                ("/", "Insert menu while typing"),
                            ] {
                                ui.label(mono(k, theme::TEXT));
                                ui.label(text(v, 12.5, theme::TEXT_2));
                                ui.end_row();
                            }
                        });
                });
        }
        if let Some(e) = &self.comments_error {
            let mut open = true;
            egui::Window::new("comments.json")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(text(e, 12.5, theme::DANGER));
                });
            if !open {
                self.comments_error = None;
            }
        }
        let _ = (project, cmds);
    }
}

/// Insert a cue at `at` with an empty label (or `label`) and select the label.
fn insert_cue(
    src: &str,
    parsed: &Script,
    at: usize,
    name: &str,
    prefix: &str,
    label: &str,
) -> (String, Range<usize>) {
    let id = edit::next_id(parsed, prefix);
    let before = src[..at].chars().next_back();
    let lead = if before.is_some_and(|c| !c.is_whitespace()) {
        " "
    } else {
        ""
    };
    let cue = format!("{lead}:{name}[{label}]{{#{id}}}");
    let out = format::insert(src, at, &cue);
    let label_at = at + lead.len() + 1 + name.len() + 1;
    (out, label_at..label_at + label.len())
}

/// Open (or keep open) the slash menu when the text before the caret is `/word`.
fn detect_slash(src: &str, caret: usize, was_open: bool) -> Option<Slash> {
    let ls = src[..caret].rfind('\n').map_or(0, |i| i + 1);
    let seg = &src[ls..caret];
    let p = seg.rfind('/')?;
    let before = seg[..p].chars().next_back();
    let query = &seg[p + 1..];
    let ok = before.is_none_or(char::is_whitespace)
        && query.len() <= 24
        && query.chars().all(|c| c.is_alphanumeric() || c == '-');
    // Open on typing the slash itself; stay open while the word grows.
    (ok && (was_open || query.is_empty())).then(|| Slash {
        start: ls + p,
        query: query.to_owned(),
        selected: 0,
    })
}

fn filtered_items(q: &str) -> Vec<SlashItem> {
    let q = q.to_lowercase();
    slash_items()
        .into_iter()
        .filter(|i| {
            q.is_empty()
                || i.label
                    .to_lowercase()
                    .split_whitespace()
                    .any(|w| w.starts_with(&q))
                || i.label.to_lowercase().contains(&q)
        })
        .collect()
}

/// The `/` insert menu at the caret. Returns the clicked item.
fn slash_menu(ctx: &egui::Context, at: Rect, s: &Slash) -> Option<usize> {
    let items = filtered_items(&s.query);
    let mut pick = None;
    egui::Area::new(Id::new("doc-slash"))
        .order(egui::Order::Foreground)
        .fixed_pos(at.left_bottom() + vec2(-6.0, 6.0))
        .show(ctx, |ui| {
            Frame::popup(ui.style())
                .inner_margin(Margin::symmetric(4, 6))
                .show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.label(mono(
                        if s.query.is_empty() {
                            "Insert".to_owned()
                        } else {
                            format!("Insert · matching \"{}\"", s.query)
                        },
                        theme::TEXT_MUTED,
                    ));
                    if items.is_empty() {
                        ui.label(muted("Nothing matches. Keep typing or press Esc."));
                    }
                    for (i, it) in items.iter().enumerate() {
                        let on = i == s.selected.min(items.len().saturating_sub(1));
                        let r = Frame::new()
                            .fill(if on {
                                theme::BG_HOVER
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (ir, _) =
                                        ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                                    icons::paint(ui.painter(), ir, it.icon, it.color);
                                    ui.label(text(it.label, 13.0, theme::TEXT));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(text(it.hint, 11.0, theme::TEXT_MUTED));
                                    });
                                });
                            })
                            .response
                            .interact(Sense::click());
                        if r.clicked() {
                            pick = Some(i);
                        }
                    }
                });
        });
    pick
}

fn cue_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    color: Color32,
    enabled: bool,
) -> egui::Response {
    let g = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(12.5), color);
    let size = vec2(g.size().x + 30.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_filled(rect, 5.0, theme::tint(color, 0.14));
    }
    let c = if enabled { color } else { theme::TEXT_MUTED };
    icons::paint(
        p,
        Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(14.0, 14.0)),
        icon,
        c,
    );
    p.galley(
        pos2(rect.left() + 24.0, rect.center().y - g.size().y / 2.0),
        g,
        c,
    );
    resp.on_hover_text(format!("Insert a {label} cue"))
}

/// A comment card at `at`; returns its rect.
fn card(
    ui: &mut egui::Ui,
    at: Pos2,
    w: f32,
    active: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> Rect {
    let mut child =
        ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(at, vec2(w, 2000.0))));
    Frame::new()
        .fill(theme::BG_PANEL)
        .stroke(Stroke::new(
            1.0,
            if active {
                theme::tint(theme::ACCENT, 0.6)
            } else {
                theme::BORDER
            },
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .shadow(if active {
            egui::epaint::Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(120),
            }
        } else {
            egui::epaint::Shadow::NONE
        })
        .show(&mut child, |ui| {
            ui.set_width(w - 24.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui);
        })
        .response
        .rect
}

fn avatar(ui: &mut egui::Ui, name: &str, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 11.0, color);
    let initial = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(11.0),
        theme::ACCENT_ON,
    );
}

fn ruler(p: &egui::Painter, r: Rect, page_x: f32, page_w: f32, z: f32) {
    p.rect_filled(r, 0.0, theme::BG_PANEL);
    p.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, theme::BORDER),
    );
    let band = Rect::from_min_max(
        pos2(page_x, r.top() + 4.0),
        pos2(page_x + page_w, r.bottom() - 4.0),
    );
    p.rect_filled(band, 1.0, theme::BG_RAISED);
    p.rect_filled(
        Rect::from_min_max(
            band.min,
            pos2(band.left() + doc::MARGIN_L * z, band.bottom()),
        ),
        0.0,
        theme::BG_INSET,
    );
    p.rect_filled(
        Rect::from_min_max(pos2(band.right() - doc::MARGIN_R * z, band.top()), band.max),
        0.0,
        theme::BG_INSET,
    );
    let inch = 90.0 * z;
    let mut k = 0;
    let mut x = page_x;
    while x <= page_x + page_w {
        let major = k % 8 == 0;
        p.line_segment(
            [
                pos2(x, band.bottom() - if major { 7.0 } else { 3.0 }),
                pos2(x, band.bottom()),
            ],
            Stroke::new(1.0, theme::TEXT_MUTED),
        );
        if major && k > 0 && x < page_x + page_w - 8.0 {
            p.text(
                pos2(x, band.top() + 1.0),
                Align2::CENTER_TOP,
                (k / 8).to_string(),
                FontId::monospace(8.5),
                theme::TEXT_MUTED,
            );
        }
        k += 1;
        x += inch / 8.0;
    }
    // Indent markers at the margins.
    for (mx, down) in [
        (page_x + doc::MARGIN_L * z, true),
        (page_x + page_w - doc::MARGIN_R * z, false),
    ] {
        let y = if down { band.top() } else { band.bottom() };
        let dy = if down { 6.0 } else { -6.0 };
        p.add(egui::Shape::convex_polygon(
            vec![pos2(mx - 5.0, y), pos2(mx + 5.0, y), pos2(mx, y + dy)],
            theme::ACCENT,
            Stroke::NONE,
        ));
    }
}

fn fmt_num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Colors offered in the color and highlight menus.
const SWATCHES: [(&str, &str); 11] = [
    ("white", "#FFFFFF"),
    ("gray", "#A2A8B0"),
    ("red", "#E5675F"),
    ("orange", "#F08C3A"),
    ("yellow", "#F2D04F"),
    ("green", "#4CC38A"),
    ("teal", "#5CC8E0"),
    ("blue", "#6F9BFF"),
    ("purple", "#A88BFA"),
    ("pink", "#E58BC0"),
    ("accent", "#F0A43A"),
];

/// A color menu. Returns `Some(None)` for "none / default", `Some(Some(color))` for a pick.
fn palette(
    ui: &mut egui::Ui,
    title: &str,
    hex: &mut String,
    highlight: bool,
) -> Option<Option<String>> {
    let mut out = None;
    ui.label(strong(title, 12.5));
    if ui
        .button(if highlight {
            "None"
        } else {
            "Default (from the style)"
        })
        .clicked()
    {
        out = Some(None);
    }
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(200.0);
        for (name, h) in SWATCHES {
            let c = doc::color_of(h).unwrap_or(Color32::WHITE);
            let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
            let fill = if highlight { theme::tint(c, 0.6) } else { c };
            ui.painter().rect(
                r,
                4.0,
                fill,
                Stroke::new(
                    if resp.hovered() { 2.0 } else { 1.0 },
                    if resp.hovered() {
                        theme::TEXT
                    } else {
                        theme::BORDER_STRONG
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if resp.on_hover_text(name).clicked() {
                out = Some(Some(name.to_owned()));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(hex)
                .hint_text("#RRGGBB")
                .desired_width(80.0),
        );
        if ui.button("Use").clicked() && doc::color_of(hex.trim()).is_some() {
            out = Some(Some(hex.trim().to_owned()));
        }
    });
    out
}

/// The font menu: bundled fonts, the project's `fonts/` folder and installed fonts, with a
/// search box. Returns the picked font.
fn font_picker(
    ui: &mut egui::Ui,
    id: &str,
    current: &str,
    width: f32,
    filter: &mut String,
) -> Option<String> {
    use crate::fonts::{self, Source};
    let mut pick = None;
    let shown = if current.is_empty() {
        "Monospace"
    } else {
        current
    };
    let missing = !current.is_empty() && !fonts::exists(current);
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(RichText::new(shown).color(if missing { theme::WARN } else { theme::TEXT }))
        .height(380.0)
        .show_ui(ui, |ui| {
            ui.set_min_width(240.0);
            ui.add(egui::TextEdit::singleline(filter).hint_text("Search fonts").desired_width(f32::INFINITY));
            let (list, scanning) = fonts::list();
            let q = filter.trim().to_lowercase();
            let mut last: Option<Source> = None;
            for (name, src) in list.iter().filter(|(n, _)| q.is_empty() || n.to_lowercase().contains(&q)) {
                if last != Some(*src) {
                    ui.label(mono(
                        match src {
                            Source::Bundled => "BUNDLED",
                            Source::Project => "THIS PROJECT (fonts/)",
                            Source::System => "INSTALLED",
                        },
                        theme::TEXT_MUTED,
                    ));
                    last = Some(*src);
                }
                // Preview bundled and project fonts in their own face.
                let fam = (*src != Source::System)
                    .then(|| fonts::ready(ui.ctx(), name))
                    .flatten()
                    .map(|_| fonts::family(name, fonts::Face::Regular))
                    .unwrap_or(FontFamily::Name(theme::DOC.into()));
                if ui
                    .selectable_label(name.eq_ignore_ascii_case(current), RichText::new(name).font(FontId::new(14.0, fam)))
                    .clicked()
                {
                    pick = Some(name.clone());
                }
            }
            if scanning {
                ui.label(muted("Looking for installed fonts…"));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            }
            if missing {
                ui.label(text(format!("\u{201C}{current}\u{201D} isn't on this computer; showing the default font."), 11.5, theme::WARN));
            }
        });
    pick
}

fn style_color(s: Style) -> Color32 {
    match s {
        Style::Vo => theme::CUE_VO,
        Style::Live => theme::CUE_LIVE,
        Style::OnScreen | Style::Note => theme::TEXT_2,
        _ => theme::TEXT,
    }
}

/// A style name drawn in its own style, for the style menu.
fn style_preview(s: Style) -> RichText {
    let (size, fam) = match s {
        Style::Title => (20.0, theme::DOC_BOLD),
        Style::Heading1 => (17.0, theme::DOC_BOLD),
        Style::Heading2 => (15.0, theme::DOC_SEMIBOLD),
        Style::Heading3 => (13.5, theme::DOC_SEMIBOLD),
        Style::OnScreen | Style::Note => (13.5, theme::DOC_ITALIC),
        _ => (13.5, theme::DOC),
    };
    RichText::new(s.label())
        .font(FontId::new(size, FontFamily::Name(fam.into())))
        .color(style_color(s))
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// The name comments are signed with.
fn author() -> String {
    let raw = std::env::var("SPEARFIA_AUTHOR")
        .or_else(|_| std::env::var("USERNAME"))
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "You".into());
    let mut c = raw.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => "You".into(),
    }
}

/// Set `key: value` in the front matter, adding the block or the key if missing.
fn set_front_matter(src: &str, key: &str, value: &str) -> String {
    let line = format!("{key}: {value}");
    let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
    if src.starts_with("---") {
        let mut out = String::new();
        let mut done = false;
        for (i, l) in src.split_inclusive('\n').enumerate() {
            let body = l.trim_end_matches(['\n', '\r']);
            if !done && i > 0 && body.starts_with(&format!("{key}:")) {
                out.push_str(&line);
                out.push_str(&l[body.len()..]);
                done = true;
                continue;
            }
            if !done && i > 0 && body.trim_end() == "---" {
                out.push_str(&line);
                out.push_str(nl);
                done = true;
            }
            out.push_str(l);
        }
        return out;
    }
    format!("---{nl}{line}{nl}---{nl}{src}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_that_failed_to_load_is_never_saved() {
        let root = std::env::temp_dir().join(format!("spearfia-buf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // A folder where the script should be: reading it fails.
        std::fs::create_dir_all(root.join("script/script.md")).unwrap();
        let project = Project {
            file: serde_json::from_value(serde_json::json!({
                "name": "t", "spec_version": 1, "adapter": "filmcraft"
            }))
            .unwrap(),
            root: root.clone(),
        };
        let mut b = Buf::load(&project, "script/script.md", true);
        assert!(b.load_failed && b.source.is_empty());
        b.source = "typed into the empty page".into();
        b.changed(1.0);
        b.save(&project);
        assert!(
            b.error
                .as_deref()
                .is_some_and(|e| e.starts_with("Not saved"))
        );
        assert!(
            root.join("script/script.md").is_dir(),
            "nothing was written"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slash_opens_on_typing() {
        let s = detect_slash("hello /", 7, false).unwrap();
        assert_eq!((s.start, s.query.as_str()), (6, ""));
        let s = detect_slash("hello /mus", 10, true).unwrap();
        assert_eq!(s.query, "mus");
        assert!(
            detect_slash("hello /mus", 10, false).is_none(),
            "doesn't reopen on old text"
        );
        assert!(detect_slash("a/b", 3, true).is_none(), "not inside a word");
        assert_eq!(filtered_items("mus")[0].label, "Music cue");
    }

    #[test]
    fn inserts_cues_with_ids() {
        let src = "Hello there\n";
        let parsed = spearfia_script::parse(src);
        let (out, sel) = insert_cue(src, &parsed, 11, "sfx", "s", "");
        assert_eq!(out, "Hello there :sfx[]{#s1}\n");
        assert_eq!(sel, 17..17);
        let (out, sel) = insert_cue(src, &parsed, 0, "look", "l", "chapter-card");
        assert_eq!(&out[sel], "chapter-card");
    }

    #[test]
    fn front_matter_titles() {
        assert_eq!(
            set_front_matter("---\ntitle: A\n---\nx\n", "title", "B"),
            "---\ntitle: B\n---\nx\n"
        );
        assert_eq!(
            set_front_matter("---\nformat: long\n---\n", "title", "B"),
            "---\nformat: long\ntitle: B\n---\n"
        );
        assert_eq!(
            set_front_matter("x\n", "title", "B"),
            "---\ntitle: B\n---\nx\n"
        );
    }
}
