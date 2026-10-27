//! Authenticated loopback JSON API. MCP clients use the separate stdio transport.
use super::{MAX_FRAME, Server, known_tool, tools};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};
const MAX_HEADERS: usize = 16384;
struct Fault {
    status: u16,
    message: &'static str,
}
fn fault(status: u16, message: &'static str) -> Fault {
    Fault { status, message }
}
struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}
fn content_length(value: &str) -> Result<usize, Fault> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(fault(400, "Invalid Content-Length."));
    }
    value
        .parse()
        .map_err(|_| fault(400, "Invalid Content-Length."))
}
fn equal_secret(a: &[u8], b: &[u8]) -> bool {
    let mut difference = a.len() ^ b.len();
    for (x, y) in a.iter().zip(b) {
        difference |= usize::from(x ^ y);
    }
    difference == 0
}
fn header<'a>(headers: &'a [httparse::Header<'a>], name: &str) -> Result<Option<&'a str>, Fault> {
    let mut values = headers.iter().filter(|h| h.name.eq_ignore_ascii_case(name));
    let value = values.next();
    if values.next().is_some() {
        return Err(fault(400, "Duplicate request header."));
    }
    value
        .map(|h| std::str::from_utf8(h.value).map_err(|_| fault(400, "Invalid header encoding.")))
        .transpose()
}
fn read_request(stream: &mut TcpStream, port: u16, token: &str) -> Result<Request, Fault> {
    let started = Instant::now();
    let mut bytes = Vec::new();
    let (method, path, length, start) = loop {
        if started.elapsed() > Duration::from_secs(3) {
            return Err(fault(408, "Request timed out."));
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(&bytes) {
            Ok(httparse::Status::Complete(start)) => {
                if start > MAX_HEADERS {
                    return Err(fault(431, "Request headers exceed 16 KiB."));
                }
                let host = header(request.headers, "Host")?
                    .ok_or_else(|| fault(400, "Host header required."))?;
                if ![format!("127.0.0.1:{port}"), format!("localhost:{port}")]
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(host))
                {
                    return Err(fault(403, "Host is not this loopback server."));
                }
                if let Some(origin) = header(request.headers, "Origin")?
                    && origin != format!("http://127.0.0.1:{port}")
                    && origin != format!("http://localhost:{port}")
                {
                    return Err(fault(403, "Origin is not allowed."));
                }
                let bearer = header(request.headers, "Authorization")?
                    .and_then(|s| s.strip_prefix("Bearer "))
                    .unwrap_or("");
                if !equal_secret(bearer.as_bytes(), token.as_bytes()) {
                    return Err(fault(401, "A valid bearer token is required."));
                }
                if header(request.headers, "Transfer-Encoding")?.is_some() {
                    return Err(fault(
                        400,
                        "Use Content-Length; chunked requests are not supported.",
                    ));
                }
                let method = request
                    .method
                    .ok_or_else(|| fault(400, "Missing request method."))?
                    .to_owned();
                let path = request
                    .path
                    .ok_or_else(|| fault(400, "Missing request path."))?
                    .to_owned();
                let length = header(request.headers, "Content-Length")?
                    .map(content_length)
                    .transpose()?
                    .unwrap_or(0);
                if length > MAX_FRAME {
                    return Err(fault(413, "Request body exceeds 4 MiB."));
                }
                if method == "POST" {
                    if header(request.headers, "Content-Length")?.is_none() {
                        return Err(fault(411, "Content-Length required."));
                    }
                    let kind = header(request.headers, "Content-Type")?
                        .unwrap_or("")
                        .split(';')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !kind.eq_ignore_ascii_case("application/json") {
                        return Err(fault(415, "Use Content-Type: application/json."));
                    }
                }
                break (method, path, length, start);
            }
            Ok(httparse::Status::Partial) => {}
            Err(_) => return Err(fault(400, "Malformed HTTP request.")),
        }
        if bytes.len() >= MAX_HEADERS {
            return Err(fault(431, "Request headers exceed 16 KiB."));
        }
        let mut chunk = [0u8; 1024];
        let count = stream
            .read(&mut chunk)
            .map_err(|_| fault(408, "Request timed out or disconnected."))?;
        if count == 0 {
            return Err(fault(400, "Incomplete request headers."));
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    let end = start
        .checked_add(length)
        .ok_or_else(|| fault(413, "Request length overflow."))?;
    while bytes.len() < end {
        if started.elapsed() > Duration::from_secs(3) {
            return Err(fault(408, "Request timed out."));
        }
        let mut chunk = [0u8; 8192];
        let take = (end - bytes.len()).min(chunk.len());
        let count = stream
            .read(&mut chunk[..take])
            .map_err(|_| fault(408, "Request timed out or disconnected."))?;
        if count == 0 {
            return Err(fault(400, "Incomplete request body."));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(Request {
        method,
        path,
        body: bytes
            .get(start..end)
            .ok_or_else(|| fault(400, "Invalid request body."))?
            .to_vec(),
    })
}
fn response(stream: &mut TcpStream, status: u16, body: Value) -> io::Result<()> {
    let body = serde_json::to_vec(&body)?;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}
fn route(server: &mut Server, request: Request) -> Result<Value, Fault> {
    if request.method == "GET" && request.path == "/v1/tools" {
        return tools().map_err(|_| fault(500, "Unable to describe tools."));
    }
    if request.method != "POST" {
        return Err(fault(405, "Use GET /v1/tools or POST /v1/tools/<name>."));
    }
    let name = request
        .path
        .strip_prefix("/v1/tools/")
        .filter(|n| known_tool(n))
        .ok_or_else(|| fault(404, "Unknown tool route."))?;
    let arguments: Value =
        serde_json::from_slice(&request.body).map_err(|_| fault(400, "Invalid JSON body."))?;
    Ok(server.tool_result(name, arguments))
}
fn connection(
    server: &mut Server,
    stream: &mut TcpStream,
    port: u16,
    token: &str,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_request(stream, port, token).and_then(|request| route(server, request))
    }));
    match result {
        Ok(Ok(body)) => response(stream, 200, body),
        Ok(Err(error)) => response(stream, error.status, json!({"error":error.message})),
        Err(_) => response(
            stream,
            500,
            json!({"error":"Request failed unexpectedly. Read documents before retrying."}),
        ),
    }
}
/// Sequential, bounded requests; writes go through `spearfia-ops` under the project edit lock.
pub fn serve(server: &mut Server, listener: TcpListener, token: &str) -> io::Result<()> {
    if token.len() < 32 || token.len() > 256 || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Set SPEARFIA_API_TOKEN to 32–256 random printable ASCII characters.",
        ));
    }
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The API must bind to loopback.",
        ));
    }
    let port = listener.local_addr()?.port();
    for stream in listener.incoming() {
        let mut stream = stream?;
        if let Err(error) = connection(server, &mut stream, port, token) {
            eprintln!("Spear Fia API connection ended: {error}");
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    const TOKEN: &str = "test-only-0123456789-0123456789-0123456789";
    fn request(headers: &str, path: &str, method: &str, body: &str) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let job = std::thread::spawn(move || {
            let mut server = crate::tests::server();
            let root = server.editor.project.root.clone();
            let (mut socket, _) = listener.accept().unwrap();
            connection(&mut server, &mut socket, address.port(), TOKEN).unwrap();
            std::fs::remove_dir_all(root).unwrap();
        });
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket.write_all(request.as_bytes()).unwrap();
        socket.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        job.join().unwrap();
        response
    }
    #[test]
    fn authenticated_http_lists_and_executes_the_same_tools() {
        let headers =
            format!("Authorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\n");
        let listed = request(&headers, "/v1/tools", "GET", "");
        assert!(listed.starts_with("HTTP/1.1 200"));
        assert!(listed.contains("document_edit"));
        let read = request(
            &headers,
            "/v1/tools/document_read",
            "POST",
            r#"{"document":"script"}"#,
        );
        assert!(read.contains("Hello world"));
        assert!(read.contains("revision"));
    }
    #[test]
    fn http_rejects_untrusted_origins_bad_auth_and_malformed_requests() {
        assert!(request("", "/v1/tools", "GET", "").starts_with("HTTP/1.1 401"));
        let auth = format!("Authorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\n");
        assert!(
            request(
                &format!("{auth}Origin: https://evil.example\r\n"),
                "/v1/tools",
                "GET",
                ""
            )
            .starts_with("HTTP/1.1 403")
        );
        assert!(
            request(
                &format!("{auth}Content-Length: 999\r\n"),
                "/v1/tools/document_read",
                "POST",
                "{}"
            )
            .starts_with("HTTP/1.1 400")
        );
        assert!(
            request(&auth, "/v1/tools/document_read", "POST", "broken").starts_with("HTTP/1.1 400")
        );
        assert!(
            request(&auth, "/v1/tools/approve_script", "POST", "{}").starts_with("HTTP/1.1 404")
        );
    }
}
