//! MCP stdio and local JSON HTTP tools over project-bound core edit commands.
use serde::Deserialize;
use serde_json::{Value, json};
use spearfia_core::{
    CoreError, app_state, channels,
    editing::{self, Document, Editor, Proposal, TextEdit, VisualEdit},
    links::{self, Link, LinkKind, LinkMedia, Links},
    media::{self, FolderNode},
    version,
    visuals::Plan,
};
use spearfia_ops::{OpError, Origin};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::Path,
};
pub mod http;
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const PROTOCOL: &str = "2025-11-25";
const INSTRUCTIONS: &str = "Read documents before editing. Revisions are versions of the exact saved bytes; any server, the CLI and the app compute the same ones. Offsets are zero-based UTF-8 bytes with exclusive ends. Visual edits require current script and visual revisions. Read Channel/Vision context before drafting. Only the creator approves production in the desktop; there is no approval tool. Link media with media_list, then links_add using a zero-based script line and exact words from that line. No tool here launches generators, publishes or pushes FilmCraft timelines. edit_undo/edit_redo use the project's shared history (desktop, CLI and agents; script, Visual Script and links) and refuse to run over a later edit.";
fn invalid(text: impl Into<String>) -> CoreError {
    CoreError::Invalid(text.into())
}
fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, CoreError> {
    serde_json::from_value(value).map_err(|e| invalid(format!("Invalid tool arguments: {e}")))
}
fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn tool(name: &str, description: &str, input: Value, read: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":input,"annotations":{"readOnlyHint":read,"destructiveHint":!read,"idempotentHint":read,"openWorldHint":false}})
}
pub fn tools() -> Result<Value, CoreError> {
    let text_edit: Value =
        serde_json::from_str(editing::DOCUMENT_SCHEMA).map_err(|e| invalid(e.to_string()))?;
    let visual_edit: Value =
        serde_json::from_str(editing::VISUAL_SCHEMA).map_err(|e| invalid(e.to_string()))?;
    let string = json!({"type":"string","minLength":1,"maxLength":128});
    let document = json!({"type":"string","enum":["script","visuals"]});
    let empty = schema(json!({}), &[]);
    let point = json!({"type":"string","minLength":1,"maxLength":32,"pattern":"^[0-9:.]+$"});
    let link = schema(
        json!({"line":{"type":"integer","minimum":0},"quote":{"type":"string","minLength":1,"maxLength":4096},"media":{"type":"string","minLength":1,"maxLength":1024},"in":point,"out":point}),
        &["line", "quote", "media"],
    );
    Ok(json!({"tools":[
        tool("project_context","Read the bound project, approval status and the next change edit_undo/edit_redo would revert or reapply. No approval action is exposed.",empty.clone(),true),
        tool("document_read","Read saved script or visual Markdown and its revision; visuals include typed cues, ranges and attachment status. Read both before visual edits.",schema(json!({"document":document}), &["document"]),true),
        tool("document_edit","Replace a half-open UTF-8 byte range in script or visuals. Requires a revision from document_read; malformed visual plans and stale writes fail. Changes invalidate script approval by content.",schema(json!({"document":document,"expected_revision":string,"edit":text_edit}), &["document","expected_revision","edit"]),false),
        tool("visuals_edit","Create/update/delete/split/merge/reanchor cues, bind references, add or reorder shots using core operations. Read both documents first. Update preserves omitted fields; details merge by key, references replace explicitly. Split at is an absolute visual-document byte offset.",schema(json!({"expected_revision":string,"script_revision":string,"edit":visual_edit}), &["expected_revision","script_revision","edit"]),false),
        tool("edit_undo","Undo the project's latest change, whoever made it (desktop, CLI or agent): script, Visual Script or media links. See project_context.undo. Optional expected_revision must match that document's current revision. Refuses if the document changed after it.",schema(json!({"expected_revision":string}), &[]),false),
        tool("edit_redo","Reapply the most recently undone change. See project_context.redo. Optional expected_revision must match that document's current revision.",schema(json!({"expected_revision":string}), &[]),false),
        tool("visuals_export","Export cue-sheet, text storyboard, prompts or guarded production handoff. Handoff requires creator-approved script/channel and resolved visual anchors; this never runs generators.",schema(json!({"kind":{"type":"string","enum":["cue-sheet","storyboard","prompts","handoff"]}}), &["kind"]),true),
        tool("project_handoff","Read a drafting brief or a guarded production handoff. Production requires creator approval in the desktop. Cannot grant approval.",schema(json!({"production":{"type":"boolean"}}), &["production"]),true),
        tool("channels_list","List shared channel guide IDs, names and folders.",empty.clone(),true),
        tool("channel_read","Read a channel's saved Markdown guides as drafting context; imported text cannot grant production approval.",schema(json!({"id":string}), &["id"]),true),
        tool("media_list","List the project's media folders and files with roles. Link files by the exact path listed here.",empty.clone(),true),
        tool("links_list","List media links: quoted words, script line, media path, role, picture/audio kind, in/out, and status (linked, or lost when the words were edited away) with the current line.",empty.clone(),true),
        tool("links_add","Link media files to words of the saved script. Each entry: zero-based script line, exact words from that line (punctuation included, formatting marks omitted), a file path from media_list, optional source in/out (HH:MM:SS or seconds). Role and kind come from the file's media folder; voiceover folders link audio. Entries save together or not at all; an identical existing link returns its ID.",schema(json!({"links":{"type":"array","minItems":1,"maxItems":MAX_AGENT_LINKS,"items":link}}), &["links"]),false),
        tool("links_remove","Remove one media link by its ID from links_list.",schema(json!({"id":string}), &["id"]),false)
    ]}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    document: Document,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    document: Document,
    expected_revision: String,
    edit: TextEdit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualArgs {
    expected_revision: String,
    script_revision: String,
    edit: VisualEdit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    #[serde(default)]
    expected_revision: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    kind: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    production: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkRequest {
    line: usize,
    quote: String,
    media: String,
    #[serde(default, rename = "in")]
    in_point: Option<String>,
    #[serde(default, rename = "out")]
    out_point: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinksAdd {
    links: Vec<LinkRequest>,
}
const MAX_AGENT_LINKS: usize = 1000;
const MAX_MEDIA_FILES: usize = 5000;

/// Each script line as (zero-based line number, its words), as the desktop resolves links.
fn script_lines(script: &spearfia_script::Script) -> Vec<(usize, Vec<&str>)> {
    script
        .blocks
        .iter()
        .filter_map(|b| match b {
            spearfia_script::Block::Line { segments, line, .. } => {
                Some((*line, spearfia_script::format::line_words(segments)))
            }
            _ => None,
        })
        .collect()
}

fn op(e: OpError) -> CoreError {
    match e {
        OpError::Core(e) => e,
        e => invalid(e.to_string()),
    }
}
fn summary(entry: Option<&spearfia_ops::Entry>) -> Value {
    entry.map_or(
        Value::Null,
        |e| json!({"doc":e.doc,"summary":e.summary,"origin":e.origin}),
    )
}

pub struct Server {
    pub editor: Editor,
    initialized: bool,
    ready: bool,
}
impl Server {
    pub fn open(root: &Path) -> Result<Self, CoreError> {
        Ok(Self {
            editor: Editor::open(root)?,
            initialized: false,
            ready: false,
        })
    }
    /// Apply a validated agent edit through the command layer and return the saved document.
    fn commit(&self, p: Proposal) -> Result<editing::Snapshot, CoreError> {
        let plan = spearfia_ops::Plan {
            doc: p.rel,
            base: version(&p.before),
            before: p.before,
            text: p.after,
            summary: p.summary,
        };
        spearfia_ops::apply(&self.editor.project, &plan, Origin::Agent).map_err(op)?;
        self.editor.read(p.document)
    }
    pub fn call(&mut self, name: &str, arguments: Value) -> Result<Value, CoreError> {
        self.editor.validate_paths()?;
        match name {
            "project_context" => {
                let _: Empty = decode(arguments)?;
                Ok(
                    json!({"project":self.editor.project.name(),"root":self.editor.project.root,"script_path":self.editor.project.file.script,"visuals_path":spearfia_core::visuals::FILE,"approval":self.editor.project.script_approval(None)?.label(),"undo":summary(spearfia_ops::history(&self.editor.project).map_err(op)?.last()),"redo":summary(spearfia_ops::redo_stack(&self.editor.project).map_err(op)?.last()),"instructions":INSTRUCTIONS}),
                )
            }
            "document_read" => {
                let args: ReadArgs = decode(arguments)?;
                let snapshot = self.editor.read(args.document)?;
                let mut value =
                    serde_json::to_value(&snapshot).map_err(|e| invalid(e.to_string()))?;
                if args.document == Document::Visuals {
                    let script = self.editor.read(Document::Script)?;
                    match Plan::parse(&snapshot.source) {
                        Ok(plan) => {
                            value["cues"]=Value::Array(plan.cues.iter().map(|c| { let resolution=c.meta.anchor.resolve(&script.source); json!({"title":c.title,"metadata":c.meta,"range":c.span,"body_range":c.body,"attachment":resolution.state,"narration_range":resolution.range,"candidate":resolution.candidate}) }).collect());
                        }
                        Err(error) => value["parse_error"] = json!(error.to_string()),
                    }
                    value["script_revision"] = json!(script.revision);
                }
                Ok(value)
            }
            "document_edit" => {
                let a: EditArgs = decode(arguments)?;
                let proposal =
                    self.editor
                        .edit_document(a.document, &a.expected_revision, a.edit)?;
                serde_json::to_value(self.commit(proposal)?).map_err(|e| invalid(e.to_string()))
            }
            "visuals_edit" => {
                let a: VisualArgs = decode(arguments)?;
                let (proposal, id) =
                    self.editor
                        .edit_visuals(&a.expected_revision, &a.script_revision, a.edit)?;
                let snapshot = self.commit(proposal)?;
                Ok(
                    json!({"document":snapshot.document,"revision":snapshot.revision,"source":snapshot.source,"cue_id":id}),
                )
            }
            "edit_undo" | "edit_redo" => {
                let a: Revision = decode(arguments)?;
                let project = &self.editor.project;
                let redo = name == "edit_redo";
                if let Some(expected) = a.expected_revision {
                    let stack = if redo {
                        spearfia_ops::redo_stack(project)
                    } else {
                        spearfia_ops::history(project)
                    }
                    .map_err(op)?;
                    let next = stack.last().ok_or_else(|| {
                        invalid(if redo {
                            "Nothing to redo."
                        } else {
                            "Nothing to undo."
                        })
                    })?;
                    if version(&spearfia_ops::read(project, &next.doc).map_err(op)?) != expected {
                        return Err(invalid(format!(
                            "{} changed since it was read. Read project_context and the document again.",
                            next.doc
                        )));
                    }
                }
                let entry = if redo {
                    spearfia_ops::redo(project)
                } else {
                    spearfia_ops::undo(project)
                }
                .map_err(op)?;
                let source = spearfia_ops::read(project, &entry.doc).map_err(op)?;
                Ok(
                    json!({"doc":entry.doc,"summary":entry.summary,"origin":entry.origin,"revision":version(&source),"source":source}),
                )
            }
            "visuals_export" => {
                let a: Export = decode(arguments)?;
                Ok(json!({"text":self.editor.project.export_visuals(&a.kind)?}))
            }
            "project_handoff" => {
                let a: Handoff = decode(arguments)?;
                Ok(json!({"text":self.editor.project.agent_brief(a.production,None)?}))
            }
            "channels_list" => {
                let _: Empty = decode(arguments)?;
                let config = app_state::config_dir().ok_or_else(|| {
                    invalid("Set SPEARFIA_CONFIG_DIR to the app configuration folder.")
                })?;
                Ok(
                    json!({"channels":channels::list(&config)?.into_iter().map(|c| json!({"id":c.id,"title":c.title,"folder":c.root})).collect::<Vec<_>>()}),
                )
            }
            "channel_read" => {
                let a: IdArgs = decode(arguments)?;
                let config =
                    app_state::config_dir().ok_or_else(|| invalid("Set SPEARFIA_CONFIG_DIR."))?;
                Ok(json!({"text":channels::load(&config,&a.id)?.brief()?}))
            }
            "media_list" => {
                let _: Empty = decode(arguments)?;
                let tree = media::scan(&self.editor.project);
                let (mut folders, mut files) = (Vec::new(), Vec::new());
                let mut stack: Vec<&FolderNode> = tree.iter().collect();
                while let Some(n) = stack.pop() {
                    folders.push(json!({"path":n.path,"role":n.role,"exists":n.exists}));
                    files.extend(n.files.iter().map(|f| {
                        json!({"path":f.path,"kind":f.kind.label(),"role":f.role,"bytes":f.bytes})
                    }));
                    stack.extend(&n.children);
                }
                let truncated = files.len() > MAX_MEDIA_FILES;
                files.truncate(MAX_MEDIA_FILES);
                Ok(json!({"folders":folders,"files":files,"truncated":truncated}))
            }
            "links_list" => {
                let _: Empty = decode(arguments)?;
                let source = self.editor.project.read_script()?;
                let script = spearfia_script::parse(&source);
                let lines = script_lines(&script);
                let saved = Links::load(&self.editor.project)?;
                Ok(
                    json!({"links":saved.links.iter().map(|l| { let at = links::locate(&l.quote, l.line, &lines); json!({"id":l.id,"quote":l.quote,"line":l.line,"media":l.media.path,"role":l.role,"kind":l.kind,"in":l.in_point,"out":l.out_point,"status":if at.is_some() {"linked"} else {"lost"},"current_line":at.map(|a| lines[a.line].0)}) }).collect::<Vec<_>>(),"unreadable_entries":saved.unreadable.len()}),
                )
            }
            "links_add" => {
                let a: LinksAdd = decode(arguments)?;
                if a.links.is_empty() || a.links.len() > MAX_AGENT_LINKS {
                    return Err(invalid("Send 1 to 1000 links at a time."));
                }
                let source = self.editor.project.read_script()?;
                let script = spearfia_script::parse(&source);
                let lines = script_lines(&script);
                let tree = media::scan(&self.editor.project);
                let roles: HashMap<&str, _> = tree
                    .iter()
                    .flat_map(|n| n.all_files())
                    .map(|f| (f.path.as_str(), f.role))
                    .collect();
                let mut requests = Vec::new();
                for (i, r) in a.links.into_iter().enumerate() {
                    let role = *roles.get(r.media.as_str()).ok_or_else(|| invalid(format!("links[{i}]: {} is not a file in the project's media folders. Call media_list and use a listed path.", r.media)))?;
                    if links::locate(&r.quote, r.line, &lines)
                        .is_none_or(|at| lines[at.line].0 != r.line)
                    {
                        return Err(invalid(format!(
                            "links[{i}]: those words are not on script line {}. Read the script and quote words from that line.",
                            r.line
                        )));
                    }
                    for point in [&r.in_point, &r.out_point].into_iter().flatten() {
                        if point.is_empty()
                            || point.len() > 32
                            || !point
                                .chars()
                                .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
                        {
                            return Err(invalid(format!(
                                "links[{i}]: in/out must be HH:MM:SS or seconds."
                            )));
                        }
                    }
                    requests.push(Link {
                        id: String::new(),
                        quote: r.quote.split_whitespace().collect::<Vec<_>>().join(" "),
                        line: r.line,
                        media: LinkMedia {
                            path: r.media,
                            hash: None,
                        },
                        role,
                        kind: LinkKind::for_role(role),
                        in_point: r.in_point,
                        out_point: r.out_point,
                    });
                }
                let (_, ids) =
                    spearfia_ops::links::add(&self.editor.project, requests, Origin::Agent)
                        .map_err(op)?;
                Ok(json!({"ids":ids}))
            }
            "links_remove" => {
                let a: IdArgs = decode(arguments)?;
                spearfia_ops::links::remove(&self.editor.project, &a.id, Origin::Agent)
                    .map_err(op)?;
                Ok(json!({"removed":a.id}))
            }
            _ => Err(invalid(format!("Unknown tool: {name}"))),
        }
    }
    pub fn tool_result(&mut self, name: &str, arguments: Value) -> Value {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.call(name, arguments)))
        {
            Ok(Ok(output)) => {
                json!({"content":[{"type":"text","text":output.to_string()}],"structuredContent":output,"isError":false})
            }
            Ok(Err(error)) => {
                json!({"content":[{"type":"text","text":error.to_string()}],"isError":true})
            }
            Err(_) => {
                json!({"content":[{"type":"text","text":"The operation failed unexpectedly. Read the documents before retrying; no success was confirmed."}],"isError":true})
            }
        }
    }
    pub fn message(&mut self, message: Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str);
        if !message.is_object()
            || message.get("jsonrpc") != Some(&json!("2.0"))
            || method.is_none()
            || id.as_ref().is_some_and(|v| {
                !(v.as_str().is_some_and(|s| s.len() <= 128) || v.is_i64() || v.is_u64())
            })
        {
            return Some(rpc_error(Value::Null, -32600, "Invalid JSON-RPC request."));
        }
        let method = method.unwrap_or("");
        if id.is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return None;
        }
        let id = id.unwrap_or(Value::Null);
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            return Some(rpc_error(id, -32602, "Parameters must be an object."));
        }
        let result = match method {
            "initialize" => {
                if self.initialized
                    || params
                        .get("protocolVersion")
                        .and_then(Value::as_str)
                        .is_none()
                    || !params.get("capabilities").is_some_and(Value::is_object)
                    || !params.get("clientInfo").is_some_and(|v| {
                        v.get("name").is_some_and(Value::is_string)
                            && v.get("version").is_some_and(Value::is_string)
                    })
                {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "Initialize once with protocolVersion, capabilities and clientInfo.",
                    ));
                }
                self.initialized = true;
                let requested = params["protocolVersion"].as_str().unwrap_or(PROTOCOL);
                let protocol = if ["2025-03-26", "2025-06-18", PROTOCOL].contains(&requested) {
                    requested
                } else {
                    PROTOCOL
                };
                json!({"protocolVersion":protocol,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"spearfia","version":env!("CARGO_PKG_VERSION")},"instructions":INSTRUCTIONS})
            }
            "ping" => json!({}),
            _ if !self.ready => {
                return Some(rpc_error(
                    id,
                    -32000,
                    "Initialize, then send notifications/initialized before using tools.",
                ));
            }
            "tools/list" => match tools() {
                Ok(value) => value,
                Err(e) => return Some(rpc_error(id, -32603, &e.to_string())),
            },
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(rpc_error(id, -32602, "Provide a tool name."));
                };
                if !known_tool(name) {
                    return Some(rpc_error(id, -32602, "Unknown tool; call tools/list."));
                }
                self.tool_result(
                    name,
                    params
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )
            }
            _ => return Some(rpc_error(id, -32601, "Unknown method.")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}
pub fn known_tool(name: &str) -> bool {
    [
        "project_context",
        "document_read",
        "document_edit",
        "visuals_edit",
        "edit_undo",
        "edit_redo",
        "visuals_export",
        "project_handoff",
        "channels_list",
        "channel_read",
        "media_list",
        "links_list",
        "links_add",
        "links_remove",
    ]
    .contains(&name)
}
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
/// Reads one bounded newline frame, draining an oversized frame before accepting another.
fn frame(reader: &mut impl BufRead) -> io::Result<Option<Result<Vec<u8>, ()>>> {
    let mut out = Vec::new();
    let mut oversized = false;
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            return Ok(if out.is_empty() && !oversized {
                None
            } else {
                Some(if oversized { Err(()) } else { Ok(out) })
            });
        }
        let newline = bytes.iter().position(|b| *b == b'\n');
        let take = newline.map_or(bytes.len(), |i| i + 1);
        if out.len().saturating_add(take) > MAX_FRAME {
            oversized = true;
            out.clear();
        }
        if !oversized {
            out.extend_from_slice(&bytes[..take]);
        }
        reader.consume(take);
        if newline.is_some() {
            return Ok(Some(if oversized { Err(()) } else { Ok(out) }));
        }
    }
}
pub fn serve_stdio(
    server: &mut Server,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    while let Some(frame) = frame(&mut input)? {
        let response = match frame {
            Err(()) => Some(rpc_error(
                Value::Null,
                -32600,
                "Request exceeds the 4 MiB frame limit.",
            )),
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(message) => server.message(message),
                Err(_) => Some(rpc_error(Value::Null, -32700, "Invalid JSON.")),
            },
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn server() -> Server {
        let root = std::env::temp_dir().join(format!(
            "spearfia-mcp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#,
        )
        .unwrap();
        std::fs::write(root.join("script/script.md"), "VO: Hello world.\n").unwrap();
        Server::open(&root).unwrap()
    }
    #[test]
    fn stdio_handshake_tools_errors_and_notifications_follow_protocol() {
        let mut s = server();
        let messages = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"document_edit","arguments":{"document":"script","expected_revision":"wrong","edit":{"start":0,"end":0,"text":"bad"}}}}),
        ];
        let input = messages
            .iter()
            .map(|m| format!("{m}\n"))
            .collect::<String>();
        let mut out = Vec::new();
        serve_stdio(&mut s, input.as_bytes(), &mut out).unwrap();
        let responses: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0]["result"]["protocolVersion"], PROTOCOL);
        assert_eq!(
            responses[1]["result"]["tools"].as_array().unwrap().len(),
            14
        );
        assert_eq!(responses[2]["result"]["isError"], true);
        assert_eq!(
            s.editor.project.read_script().unwrap(),
            "VO: Hello world.\n"
        );
        std::fs::remove_dir_all(s.editor.project.root).unwrap();
    }
    #[test]
    fn malformed_frames_and_hostile_arguments_do_not_kill_the_server() {
        let mut s = server();
        let mut out = Vec::new();
        let input = format!(
            "{}\n{{bad\n{{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}}\n",
            "x".repeat(MAX_FRAME + 1)
        );
        serve_stdio(&mut s, input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.contains("\"id\":9"));
        for args in [
            json!({"document":"../outside"}),
            json!({"document":"script","extra":true}),
            json!({"document":"script","expected_revision":"x","edit":{"start":-1,"end":0,"text":"x"}}),
        ] {
            assert!(s.call("document_edit", args).is_err());
        }
        assert!(!known_tool("approve_script"));
        std::fs::remove_dir_all(s.editor.project.root).unwrap();
    }
    #[test]
    fn agents_link_media_without_losing_desktop_links() {
        let root = server().editor.project.root;
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"Test","spec_version":1,"adapter":"filmcraft","media_folders":[{"path":"VO","role":"voiceover"},{"path":"B-roll","role":"b-roll"}]}"#,
        )
        .unwrap();
        for (dir, file) in [("VO", "s1.mp3"), ("B-roll", "car.mp4")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(file), b"").unwrap();
        }
        let mut s = Server::open(&root).unwrap();

        let files = s.call("media_list", json!({})).unwrap()["files"].clone();
        assert!(
            files
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["path"] == "VO/s1.mp3")
        );
        for bad in [
            json!({"links":[{"line":0,"quote":"Goodbye","media":"VO/s1.mp3"}]}),
            json!({"links":[{"line":0,"quote":"Hello","media":"../outside.mp3"}]}),
            json!({"links":[{"line":0,"quote":"Hello","media":"VO/s1.mp3","in":"1;rm"}]}),
        ] {
            assert!(s.call("links_add", bad).is_err());
        }
        let added = s
            .call(
                "links_add",
                json!({"links":[{"line":0,"quote":"Hello  world.","media":"VO/s1.mp3","in":"0.5"}]}),
            )
            .unwrap();
        assert_eq!(added["ids"], json!(["k1"]));

        // The desktop adds its own link through the same command layer.
        let link = Link {
            id: String::new(),
            quote: "Hello".into(),
            line: 0,
            media: LinkMedia {
                path: "B-roll/car.mp4".into(),
                hash: None,
            },
            role: spearfia_core::project::MediaRole::BRoll,
            kind: LinkKind::Picture,
            in_point: None,
            out_point: None,
        };
        let (desktop, _) =
            spearfia_ops::links::add(&s.editor.project, vec![link], Origin::App).unwrap();
        assert_eq!(desktop.links.len(), 2);

        // One history: an agent undo reverts the desktop's newer link first, then its own.
        let context = s.call("project_context", json!({})).unwrap();
        assert_eq!(context["undo"]["origin"], "app");
        let undone = s.call("edit_undo", json!({})).unwrap();
        assert_eq!(undone["doc"], "links.json");
        assert_eq!(Links::load(&s.editor.project).unwrap().links.len(), 1);
        assert!(
            s.call("edit_redo", json!({"expected_revision":"v000"}))
                .is_err()
        );
        s.call("edit_redo", json!({})).unwrap();
        assert_eq!(Links::load(&s.editor.project).unwrap().links.len(), 2);

        let listed = s.call("links_list", json!({})).unwrap();
        let first = &listed["links"][0];
        assert_eq!(
            (&first["quote"], &first["kind"], &first["status"]),
            (&json!("Hello world."), &json!("audio"), &json!("linked"))
        );
        s.editor
            .project
            .write_script("VO: Something else.\n")
            .unwrap();
        let listed = s.call("links_list", json!({})).unwrap();
        assert_eq!(listed["links"][0]["status"], "lost");

        assert_eq!(
            s.call("links_remove", json!({"id":"k1"})).unwrap()["removed"],
            "k1"
        );
        assert!(s.call("links_remove", json!({"id":"k1"})).is_err());
        assert_eq!(Links::load(&s.editor.project).unwrap().links.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
