use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use predicates::prelude::*;

struct PageServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PageServer {
    fn start_default() -> Self {
        Self::start_with_routes(default_routes)
    }

    fn start_with_robots_disallow_a() -> Self {
        let robots = "User-agent: *\nDisallow: /a\n".to_owned();
        Self::start_with_routes(|base_url| {
            let mut routes = default_routes(base_url);
            routes.insert("/robots.txt".to_owned(), robots);
            routes
        })
    }

    fn start_with_routes<F>(routes_fn: F) -> Self
    where
        F: FnOnce(&str) -> HashMap<String, String>,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let routes = routes_fn(&base_url);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).expect("set_nonblocking");

            while !stop_bg.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_conn(stream, &routes);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for PageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.base_url.trim_start_matches("http://"));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_conn(mut stream: TcpStream, routes: &HashMap<String, String>) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }

    let req = String::from_utf8_lossy(&buf[..n]);
    let path = req
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let (status, body) = routes
        .get(path)
        .map(|body| ("200 OK", body.as_str()))
        .unwrap_or(("404 Not Found", "<html><body>not found</body></html>"));

    let body_bytes = body.as_bytes();
    let content_type = if path == "/robots.txt" {
        "text/plain; charset=utf-8"
    } else {
        "text/html; charset=utf-8"
    };
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(resp.as_bytes())?;
    stream.write_all(body_bytes)?;
    stream.flush()?;
    Ok(())
}

fn default_routes(base_url: &str) -> HashMap<String, String> {
    let mut routes = HashMap::new();

    let start = format!(
        "<html><body>\
         <h1>Start</h1>\
         <p>Start page text.</p>\
         <a href=\"{base_url}/a\">Page A</a>\
         <a href=\"{base_url}/b\">Page B</a>\
         </body></html>"
    );
    let a = "<html><body><h1>A</h1><p>Alpha text.</p></body></html>".to_owned();
    let b = "<html><body><h1>B</h1><p>Beta text.</p></body></html>".to_owned();

    routes.insert("/start".to_owned(), start);
    routes.insert("/a".to_owned(), a);
    routes.insert("/b".to_owned(), b);
    routes
}

struct OpenAiMockServer {
    api_base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl OpenAiMockServer {
    fn start(start_url: String, child_url: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let api_base_url = format!("http://{addr}/v1/");

        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).expect("set_nonblocking");
            while !stop_bg.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_openai_conn(stream, &start_url, &child_url);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            api_base_url,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for OpenAiMockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(
            self.api_base_url
                .trim_start_matches("http://")
                .trim_end_matches("/v1/"),
        );
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_openai_conn(
    mut stream: TcpStream,
    start_url: &str,
    child_url: &str,
) -> std::io::Result<()> {
    let (_method, path, body) = read_http_request(&mut stream)?;
    if path != "/v1/responses" {
        return write_json(
            &mut stream,
            "404 Not Found",
            serde_json::json!({ "error": "not found" }),
        );
    }

    let request_json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let is_web_search = request_json.get("tools").is_some();

    if is_web_search {
        let resp = serde_json::json!({
            "id": "resp_test_search",
            "output": [
                {
                    "type": "web_search_call",
                    "id": "ws_test",
                    "status": "completed",
                    "action": {
                        "type": "search",
                        "sources": [
                            { "url": start_url, "title": "Start" }
                        ]
                    }
                },
                {
                    "type": "message",
                    "id": "msg_test_search",
                    "status": "completed",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": serde_json::json!({ "results": [ { "url": start_url, "title": "Start" } ] }).to_string()
                        }
                    ]
                }
            ]
        });
        return write_json(&mut stream, "200 OK", resp);
    }

    let resp = serde_json::json!({
        "id": "resp_test_select",
        "output": [
            {
                "type": "message",
                "id": "msg_test",
                "status": "completed",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": serde_json::json!({ "selected_urls": [child_url] }).to_string()
                    }
                ]
            }
        ]
    });
    write_json(&mut stream, "200 OK", resp)
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<(String, String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut header_end = None;
    while header_end.is_none() {
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = Some(pos + 4);
        }
        if buf.len() > 1024 * 128 {
            break;
        }
    }

    let header_end = header_end.unwrap_or(buf.len());
    let header_bytes = &buf[..header_end];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut lines = header_text.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let path = parts.next().unwrap_or("/").to_owned();

    let mut content_length = 0usize;
    let mut transfer_encoding_chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        }
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value.trim().eq_ignore_ascii_case("chunked")
        {
            transfer_encoding_chunked = true;
        }
    }

    let mut body = buf[header_end..].to_vec();
    if content_length > 0 {
        while body.len() < content_length {
            let mut chunk = vec![0u8; content_length - body.len()];
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
    } else if transfer_encoding_chunked {
        body = read_chunked_body(stream, body)?;
    }

    Ok((method, path, body))
}

fn read_chunked_body(stream: &mut TcpStream, mut raw: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let Some(line_end) = find_crlf(&raw) else {
            read_more(stream, &mut raw)?;
            continue;
        };
        let size_line = &raw[..line_end];
        let size_str = String::from_utf8_lossy(size_line);
        let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
        raw.drain(..line_end + 2);

        if size == 0 {
            while raw.len() < 2 {
                read_more(stream, &mut raw)?;
            }
            raw.drain(..2);
            break;
        }

        while raw.len() < size + 2 {
            read_more(stream, &mut raw)?;
        }

        decoded.extend_from_slice(&raw[..size]);
        raw.drain(..size + 2);
    }

    Ok(decoded)
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn read_more(stream: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<()> {
    let mut chunk = [0u8; 1024];
    let n = stream.read(&mut chunk)?;
    if n == 0 {
        return Ok(());
    }
    buf.extend_from_slice(&chunk[..n]);
    Ok(())
}

fn write_json(
    stream: &mut TcpStream,
    status: &str,
    json: serde_json::Value,
) -> std::io::Result<()> {
    let body = json.to_string();
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(resp.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[test]
fn spider_respects_max_pages() {
    let pages = PageServer::start_default();
    let start_url = format!("{}/start", pages.base_url);
    let child_url = format!("{}/a", pages.base_url);
    let openai = OpenAiMockServer::start(start_url.clone(), child_url);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("OPENAI_API_KEY", "test")
        .env("OPENAI_BASE_URL", &openai.api_base_url);
    cmd.args([
        "spider",
        "--query",
        "q",
        "--search-limit",
        "1",
        "--max-pages",
        "1",
        "--max-depth",
        "1",
        "--min-sources",
        "1",
        "--max-chars",
        "4000",
        "--max-elapsed",
        "30s",
        "--allow-local",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("## Sources"))
    .stdout(predicate::str::contains(&start_url))
    .stdout(predicate::str::contains("[Medium]"))
    .stdout(predicate::str::contains("/a").not())
    .stdout(predicate::str::contains("/b").not());
}

#[test]
fn spider_respects_max_depth_zero() {
    let pages = PageServer::start_default();
    let start_url = format!("{}/start", pages.base_url);
    let child_url = format!("{}/a", pages.base_url);
    let openai = OpenAiMockServer::start(start_url.clone(), child_url);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("OPENAI_API_KEY", "test")
        .env("OPENAI_BASE_URL", &openai.api_base_url);
    cmd.args([
        "spider",
        "--query",
        "q",
        "--search-limit",
        "1",
        "--max-pages",
        "10",
        "--max-depth",
        "0",
        "--min-sources",
        "1",
        "--max-chars",
        "4000",
        "--max-elapsed",
        "30s",
        "--allow-local",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(&start_url))
    .stdout(predicate::str::contains("/a").not())
    .stdout(predicate::str::contains("/b").not());
}

#[test]
fn spider_output_respects_max_chars() {
    let pages = PageServer::start_default();
    let start_url = format!("{}/start", pages.base_url);
    let child_url = format!("{}/a", pages.base_url);
    let openai = OpenAiMockServer::start(start_url.clone(), child_url);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("OPENAI_API_KEY", "test")
        .env("OPENAI_BASE_URL", &openai.api_base_url);
    cmd.args([
        "spider",
        "--query",
        "q",
        "--search-limit",
        "1",
        "--max-pages",
        "10",
        "--max-depth",
        "1",
        "--min-sources",
        "1",
        "--max-chars",
        "80",
        "--max-elapsed",
        "30s",
        "--allow-local",
    ]);

    let output = cmd.output().expect("run spider");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.chars().count() <= 80);
}

#[test]
fn spider_llm_selects_child_links() {
    let pages = PageServer::start_default();
    let start_url = format!("{}/start", pages.base_url);
    let child_url = format!("{}/a", pages.base_url);
    let openai = OpenAiMockServer::start(start_url.clone(), child_url.clone());

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("OPENAI_API_KEY", "test")
        .env("OPENAI_BASE_URL", &openai.api_base_url);
    cmd.args([
        "spider",
        "--query",
        "q",
        "--search-limit",
        "1",
        "--max-pages",
        "2",
        "--max-depth",
        "1",
        "--min-sources",
        "2",
        "--max-chars",
        "4000",
        "--max-elapsed",
        "30s",
        "--max-children-per-page",
        "1",
        "--allow-local",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(&start_url))
    .stdout(predicate::str::contains(&child_url));
}

#[test]
fn spider_respects_robots_txt_disallow() {
    let pages = PageServer::start_with_robots_disallow_a();
    let start_url = format!("{}/start", pages.base_url);
    let disallowed_url = format!("{}/a", pages.base_url);
    let allowed_url = format!("{}/b", pages.base_url);
    let openai = OpenAiMockServer::start(start_url.clone(), disallowed_url.clone());

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("OPENAI_API_KEY", "test")
        .env("OPENAI_BASE_URL", &openai.api_base_url);
    cmd.args([
        "spider",
        "--query",
        "q",
        "--search-limit",
        "1",
        "--max-pages",
        "2",
        "--max-depth",
        "1",
        "--min-sources",
        "2",
        "--max-chars",
        "4000",
        "--max-elapsed",
        "30s",
        "--allow-local",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(&start_url))
    .stdout(predicate::str::contains(&allowed_url))
    .stdout(predicate::str::contains(&disallowed_url).not());
}
