// SPDX-License-Identifier: MIT OR Apache-2.0
//! Native fixture for runner integration tests; never shipped as an application binary.
use std::{io::{self, Read}, path::PathBuf};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).unwrap();
    if prompt.contains("FAKE_WAIT") { std::thread::sleep(std::time::Duration::from_secs(30)); }
    let question = prompt.contains("FAKE_QUESTION") && !prompt.contains("Creator response: yes");
    let report = if question {
        r#"{"status":"needs_input","summary":"Voice is missing","question":"Use the default voice?"}"#
    } else { r#"{"status":"completed","summary":"Saved outputs","question":""}"# };
    if !question && !prompt.contains("FAKE_MISSING") {
        std::fs::create_dir_all("production").unwrap();
        std::fs::write("production/out.md", "verified fixture output\n").unwrap();
    }
    if prompt.contains("FAKE_FAIL") { eprintln!("fixture failure"); std::process::exit(3); }
    if let Some(at) = args.iter().position(|a| a == "--output-last-message") {
        std::fs::write(PathBuf::from(&args[at + 1]), report).unwrap();
        if prompt.contains("FAKE_PROVIDER_ERROR") { println!(r#"{{"type":"turn.failed","error":{{"message":"fixture error"}}}}"#); }
        else { println!(r#"{{"type":"turn.completed"}}"#); }
    } else if prompt.contains("FAKE_DENIAL") {
        println!(r#"{{"type":"result","is_error":false,"permission_denials":[{{"tool_name":"Bash"}}],"structured_output":{report}}}"#);
    } else {
        println!(r#"{{"type":"result","is_error":false,"structured_output":{report}}}"#);
    }
}
