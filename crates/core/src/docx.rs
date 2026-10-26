//! `.docx` → `script.md`. A `.docx` is a zip holding `word/document.xml`; this walks its
//! paragraphs, runs and tables and writes the Markdown the Script page uses:
//! Title → `#`, Heading 1–3 → `##`–`####`, bold / italic / underline / strike →
//! `**` / `*` / `++` / `~~`, list items → `- `, tables → `| a | b |`.
//!
//! Images, footnotes, comments and fonts are not carried over; [`Converted::skipped`] says
//! which of them the document had, so the caller can tell the user.

use crate::error::{CoreError, Result};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::io::Read;
use std::path::Path;

pub struct Converted {
    pub markdown: String,
    /// Things in the document that were left out (`"3 images"`, `"comments"`).
    pub skipped: Vec<String>,
}

/// Read a `.docx` file and convert it.
pub fn import(path: &Path) -> Result<Converted> {
    let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let bad = |what: &str| {
        CoreError::Invalid(format!(
            "{} isn't a readable .docx ({what}).",
            path.display()
        ))
    };
    let mut zip = zip::ZipArchive::new(file).map_err(|_| bad("not a zip file"))?;
    let has_comments = zip.by_name("word/comments.xml").is_ok();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|_| bad("no word/document.xml"))?
        .read_to_string(&mut xml)
        .map_err(|e| CoreError::io(path, e))?;
    let mut out = convert(&xml).map_err(|e| bad(&e))?;
    if has_comments {
        out.skipped.push("comments".into());
    }
    Ok(out)
}

#[derive(Default)]
struct Run {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

impl Run {
    fn wrap(&self, text: &str) -> String {
        // Marks hug the words: `**bold** ` not `**bold **`.
        let core = text.trim();
        if core.is_empty() {
            return text.to_owned();
        }
        let mut m = String::new();
        for (on, mark) in [
            (self.bold, "**"),
            (self.italic, "*"),
            (self.underline, "++"),
            (self.strike, "~~"),
        ] {
            if on {
                m.push_str(mark);
            }
        }
        let close: String = m.chars().rev().collect();
        let lead = &text[..text.len() - text.trim_start().len()];
        let tail = &text[text.trim_end().len()..];
        format!("{lead}{m}{core}{close}{tail}")
    }
}

#[derive(Default)]
struct Para {
    style: String,
    list: bool,
    text: String,
}

impl Para {
    fn markdown(&self) -> Option<String> {
        let t = self.text.trim();
        if t.is_empty() {
            return None;
        }
        let prefix = match self.style.as_str() {
            "Title" => "# ",
            "Heading1" => "## ",
            "Heading2" => "### ",
            "Heading3" | "Heading4" | "Heading5" | "Heading6" => "#### ",
            _ if self.list => "- ",
            _ => "",
        };
        Some(format!("{prefix}{t}"))
    }
}

fn val(e: &BytesStart) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == "val")
        .map(|a| a.value.into_owned())
}

/// A run property like `<w:b/>` is on unless `w:val` is `0` / `false` / `none`.
fn flag(e: &BytesStart) -> bool {
    !matches!(val(e).as_deref(), Some("0" | "false" | "none"))
}

/// Convert `word/document.xml` text.
pub fn convert(xml: &str) -> std::result::Result<Converted, String> {
    let mut reader = Reader::from_str(xml);
    let mut lines: Vec<String> = Vec::new();
    let mut images = 0usize;
    let mut para: Option<Para> = None;
    let mut run = Run::default();
    let mut in_text = false;
    let mut run_text = String::new();
    // Tables: rows of cells, each cell the text of its paragraphs.
    let mut table_depth = 0usize;
    let mut row: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cell = String::new();

    loop {
        let ev = reader.read_event().map_err(|e| e.to_string())?;
        match &ev {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    "p" if !empty => para = Some(Para::default()),
                    "pStyle" => {
                        if let Some(p) = &mut para {
                            p.style = val(e).unwrap_or_default();
                        }
                    }
                    "numPr" => {
                        if let Some(p) = &mut para {
                            p.list = true;
                        }
                    }
                    "r" if !empty => {
                        run = Run::default();
                        run_text.clear();
                    }
                    "b" => run.bold = flag(e),
                    "i" => run.italic = flag(e),
                    "u" => run.underline = flag(e),
                    "strike" | "dstrike" => run.strike = flag(e),
                    "t" if !empty => in_text = true,
                    "tab" => run_text.push(' '),
                    "br" | "cr" => run_text.push(' '),
                    "drawing" | "pict" if !empty => images += 1,
                    "tbl" if !empty => {
                        table_depth += 1;
                        if table_depth == 1 {
                            rows.clear();
                        }
                    }
                    "tr" if !empty => row.clear(),
                    "tc" if !empty => cell.clear(),
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                run_text.push_str(t);
            }
            Event::GeneralRef(r) if in_text => {
                let name: &str = r;
                let ch = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => r.resolve_char_ref().ok().flatten(),
                };
                if let Some(c) = ch {
                    run_text.push(c);
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "t" => in_text = false,
                "r" => {
                    if let Some(p) = &mut para {
                        p.text.push_str(&run.wrap(&run_text));
                    }
                }
                "p" => {
                    if let Some(p) = para.take() {
                        if table_depth > 0 {
                            if !cell.is_empty() && !p.text.trim().is_empty() {
                                cell.push(' ');
                            }
                            cell.push_str(p.text.trim());
                        } else if let Some(md) = p.markdown() {
                            // A blank line before each heading keeps sections readable.
                            if md.starts_with('#') && !lines.is_empty() {
                                lines.push(String::new());
                            }
                            lines.push(md);
                        }
                    }
                }
                "tc" => row.push(cell.replace('|', "\\|")),
                "tr" => rows.push(std::mem::take(&mut row)),
                "tbl" => {
                    table_depth = table_depth.saturating_sub(1);
                    if table_depth == 0 && !rows.is_empty() {
                        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
                        for (i, r) in rows.iter().enumerate() {
                            let cells: Vec<&str> = (0..cols)
                                .map(|c| r.get(c).map_or("", String::as_str))
                                .collect();
                            lines.push(format!("| {} |", cells.join(" | ")));
                            if i == 0 {
                                lines.push(format!("|{}", "---|".repeat(cols)));
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    let mut skipped = Vec::new();
    if images > 0 {
        skipped.push(format!(
            "{images} image{}",
            if images == 1 { "" } else { "s" }
        ));
    }
    let mut markdown = lines.join("\n");
    markdown.push('\n');
    Ok(Converted { markdown, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn doc(body: &str) -> String {
        format!(r#"<?xml version="1.0"?><w:document {W}><w:body>{body}</w:body></w:document>"#)
    }

    fn p(style: &str, runs: &str) -> String {
        let ppr = if style.is_empty() {
            String::new()
        } else {
            format!(r#"<w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#)
        };
        format!("<w:p>{ppr}{runs}</w:p>")
    }

    fn r(props: &str, text: &str) -> String {
        format!(r#"<w:r><w:rPr>{props}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
    }

    #[test]
    fn headings_marks_lists_and_entities() {
        let body = [
            p("Title", &r("", "Learning to Drift")),
            p("Heading1", &r("", "Cold Open")),
            p("", &(r("", "I spent ") + &r("<w:b/>", "eight hours ") + &r("<w:i/><w:u w:val=\"single\"/>", "crashing"))),
            p("", &r("<w:b w:val=\"0\"/>", "Tom &amp; Jerry &lt;3")),
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>point one</w:t></w:r></w:p>"#.to_owned(),
            p("Heading2", &r("<w:strike/>", "old")),
        ]
        .concat();
        let out = convert(&doc(&body)).unwrap();
        assert_eq!(
            out.markdown,
            "# Learning to Drift\n\n## Cold Open\nI spent **eight hours** *++crashing++*\nTom & Jerry <3\n- point one\n\n### ~~old~~\n"
        );
        assert!(out.skipped.is_empty());
    }

    #[test]
    fn tables_and_images() {
        let cell = |t: &str| format!("<w:tc>{}</w:tc>", p("", &r("", t)));
        let body = format!(
            "<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>{}",
            cell("Shot"),
            cell("Note"),
            cell("A|B"),
            cell("wide"),
            p("", "<w:r><w:drawing></w:drawing></w:r>")
        );
        let out = convert(&doc(&body)).unwrap();
        assert_eq!(
            out.markdown,
            "| Shot | Note |\n|---|---|\n| A\\|B | wide |\n"
        );
        assert_eq!(out.skipped, ["1 image"]);
    }

    #[test]
    fn rejects_files_that_are_not_docx() {
        let dir = crate::testutil::temp_dir("docx");
        let path = dir.join("fake.docx");
        std::fs::write(&path, "plain text").unwrap();
        let err = import(&path).err().unwrap().to_string();
        assert!(err.contains("isn't a readable .docx"), "{err}");
    }

    #[test]
    fn reads_a_real_zip() {
        use std::io::Write;
        let dir = crate::testutil::temp_dir("docx-zip");
        let path = dir.join("s.docx");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        z.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        z.write_all(doc(&p("Heading1", &r("", "Hook"))).as_bytes())
            .unwrap();
        z.start_file(
            "word/comments.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        z.finish().unwrap();
        let out = import(&path).unwrap();
        assert_eq!(out.markdown, "## Hook\n");
        assert_eq!(out.skipped, ["comments"]);
    }
}
