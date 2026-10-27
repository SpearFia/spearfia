//! Real-process tests: the shipped CLI, not an in-process mock transport.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::Duration,
};
const SCRIPT: &str = "LIVE: Hello world.\nVO: Welcome back.\n";
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("spearfia-wire-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::create_dir_all(root.join("sync")).unwrap();
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"Wire test","spec_version":1,"adapter":"filmcraft"}"#,
        )
        .unwrap();
        std::fs::write(root.join("script/script.md"), SCRIPT).unwrap();
        std::fs::write(root.join("vision.yaml"), "{}").unwrap();
        std::fs::write(root.join("channel.md"), "# Test channel\n").unwrap();
        std::fs::write(root.join("sync/approved-script.md"), SCRIPT).unwrap();
        std::fs::write(root.join("sync/approved-channel.md"), "# Test channel\n").unwrap();
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Mcp {
    process: Process,
    input: Option<ChildStdin>,
    output: Receiver<Value>,
    next: u64,
}
impl Mcp {
    fn new(f: &Fixture) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_spearfia"))
            .arg("mcp")
            .arg("--project")
            .arg(&f.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let value: Value =
                    serde_json::from_str(&line).expect("stdout must contain only MCP JSON");
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            process: Process(child),
            input: Some(input),
            output,
            next: 0,
        }
    }
    fn send(&mut self, message: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        self.send(json!({"jsonrpc":"2.0","id":self.next,"method":method,"params":params}));
        let reply = self
            .output
            .recv_timeout(Duration::from_secs(5))
            .expect("MCP response deadline");
        assert_eq!(reply["id"], self.next);
        reply
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let reply = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(reply["result"]["isError"], false, "{reply}");
        reply["result"]["structuredContent"].clone()
    }
    fn visual(&mut self, revision: &Value, script: &Value, edit: Value) -> Value {
        self.tool(
            "visuals_edit",
            json!({"expected_revision":revision,"script_revision":script,"edit":edit}),
        )
    }
}
#[test]
fn real_mcp_roundtrip_edits_every_visual_operation_and_checks_conflicts() {
    let f = Fixture::new();
    let mut client = Mcp::new(&f);
    let init=client.request("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"wire-test","version":"1"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert!(
        client
            .request("tools/list", json!({}))
            .get("error")
            .is_some()
    );
    client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let listed = client.request("tools/list", json!({}));
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 23);
    let context = client.tool("project_context", json!({}));
    let original_approval = context["approval"].clone();
    let s = client.tool("document_read", json!({"document":"script"}));
    let v = client.tool("document_read", json!({"document":"visuals"}));
    let v = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"create","title":"Scene","start":0,"end":SCRIPT.find('\n').unwrap()}),
    );
    let id = v["cue_id"].clone();
    let stale = v["revision"].clone();
    let body =
        "VISUAL: Garden.\n\n### Shot 1\nVISUAL: Wide view.\n\n### Shot 2\nPROMPT: Root insert.\n";
    let v=client.visual(&v["revision"],&s["revision"],json!({"action":"update","id":id,"patch":{"title":"Garden scene","body":body,"visual_type":"Character scene","method":"Remotion template","details":{"Characters":"Maya and Jess"}}}));
    let v=client.visual(&v["revision"],&s["revision"],json!({"action":"bind_reference","id":id,"reference":{"role":"style","path":"art.svg","revision":"approved-2","scope":"use"}}));
    let v = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"add_shot","id":id}),
    );
    let v = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"move_shot","id":id,"from":2,"to":0}),
    );
    let at = v["source"].as_str().unwrap().find("### Shot 1").unwrap();
    let v = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"split","id":id,"at":at}),
    );
    let v = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"merge","id":id}),
    );
    let v=client.visual(&v["revision"],&s["revision"],json!({"action":"reanchor","id":id,"start":SCRIPT.find("VO:").unwrap(),"end":SCRIPT.trim_end().len()}));
    let stale_reply=client.request("tools/call",json!({"name":"visuals_edit","arguments":{"expected_revision":stale,"script_revision":s["revision"],"edit":{"action":"delete","id":id}}}));
    assert_eq!(stale_reply["result"]["isError"], true);
    let before = v["source"].clone();
    let deleted = client.visual(
        &v["revision"],
        &s["revision"],
        json!({"action":"delete","id":id}),
    );
    let restored = client.tool(
        "edit_undo",
        json!({"expected_revision":deleted["revision"]}),
    );
    assert_eq!(restored["source"], before);
    let redone = client.tool(
        "edit_redo",
        json!({"expected_revision":restored["revision"]}),
    );
    assert_ne!(redone["source"], before);
    let restored = client.tool("edit_undo", json!({"expected_revision":redone["revision"]}));
    assert_eq!(restored["source"], before);
    let read = client.tool("document_read", json!({"document":"visuals"}));
    assert_eq!(
        read["cues"][0]["metadata"]["references"][0]["revision"],
        "approved-2"
    );
    let edit=client.tool("document_edit",json!({"document":"script","expected_revision":s["revision"],"edit":{"start":0,"end":0,"text":"VO: New opening.\n"}}));
    assert!(
        edit["source"]
            .as_str()
            .unwrap()
            .starts_with("VO: New opening.")
    );
    let context = client.tool("project_context", json!({}));
    assert_ne!(context["approval"], original_approval);
    let production = client.request(
        "tools/call",
        json!({"name":"project_handoff","arguments":{"production":true}}),
    );
    assert_eq!(production["result"]["isError"], true);
    assert!(
        client
            .request(
                "tools/call",
                json!({"name":"approve_script","arguments":{}})
            )
            .get("error")
            .is_some()
    );
    drop(client.input.take());
    assert!(client.process.0.wait().unwrap().success());
}
fn http(port: u16, token: &str, path: &str, args: Option<Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = args.as_ref().map_or(String::new(), Value::to_string);
    let method = if args.is_some() { "POST" } else { "GET" };
    write!(stream,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    let (headers, body) = out.split_once("\r\n\r\n").unwrap();
    (
        headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}
#[test]
fn real_http_api_authenticates_and_edits_without_mcp_handshake() {
    let f = Fixture::new();
    let token = "test-only-random-fixture-0123456789abcdef";
    let mut child = Command::new(env!("CARGO_BIN_EXE_spearfia"))
        .args(["api", "--project"])
        .arg(&f.root)
        .args(["--port", "0"])
        .env("SPEARFIA_API_TOKEN", token)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut process = Process(child);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stderr).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let startup = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("API startup deadline");
    assert!(!startup.contains(token));
    let port: u16 = startup
        .split("127.0.0.1:")
        .nth(1)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(http(port, "wrong", "/v1/tools", None).0, 401);
    let (_, tools) = http(port, token, "/v1/tools", None);
    assert_eq!(tools["tools"].as_array().unwrap().len(), 23);
    let (_, read) = http(
        port,
        token,
        "/v1/tools/document_read",
        Some(json!({"document":"script"})),
    );
    let revision = read["structuredContent"]["revision"].clone();
    let arguments = json!({"document":"script","expected_revision":revision,"edit":{"start":0,"end":0,"text":"VO: HTTP edit.\n"}});
    let (status, edited) = http(
        port,
        token,
        "/v1/tools/document_edit",
        Some(arguments.clone()),
    );
    assert_eq!(status, 200);
    assert_eq!(edited["isError"], false);
    assert_eq!(
        http(port, token, "/v1/tools/document_edit", Some(arguments)).1["isError"],
        true
    );
    assert!(
        std::fs::read_to_string(f.root.join("script/script.md"))
            .unwrap()
            .starts_with("VO: HTTP edit.")
    );
    process.0.kill().unwrap();
    process.0.wait().unwrap();
}
