//! Static file server for local development of the browser application.
//!
//! The browser app must be served over HTTP rather than opened from disk: it
//! loads ES modules, spawns a Web Worker, and instantiates WASM, all of which
//! are blocked under `file://`. This binary exists so that requirement costs a
//! `cargo run` and no non-Rust toolchain.
//!
//! It is a development tool. It binds loopback only, serves `GET`/`HEAD`, and
//! is not written to face a network.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

const READ_LIMIT: u64 = 64 * 1024;

fn main() {
    let mut port = 8000_u16;
    let mut root = PathBuf::from(".");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                if let Some(value) = args.next().and_then(|v| v.parse().ok()) {
                    port = value;
                }
            }
            "--root" => {
                if let Some(value) = args.next() {
                    root = PathBuf::from(value);
                }
            }
            "--help" | "-h" => {
                println!("ccm-serve [--port N] [--root DIR]");
                return;
            }
            _ => {}
        }
    }

    let root = match fs::canonicalize(&root) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("cannot resolve --root {}: {error}", root.display());
            std::process::exit(1);
        }
    };
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot bind 127.0.0.1:{port}: {error}");
            std::process::exit(1);
        }
    };
    println!("serving {} at http://127.0.0.1:{port}", root.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle(stream, &root) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        }
    }
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.by_ref().take(READ_LIMIT).read_line(&mut request)? == 0 {
        return Ok(());
    }
    // Drain the headers so the client sees a clean response rather than a reset.
    let mut header = String::new();
    while reader.by_ref().take(READ_LIMIT).read_line(&mut header)? > 2 {
        header.clear();
    }

    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed", true);
    }

    let Some(path) = resolve(root, target) else {
        return respond(&mut stream, 404, "text/plain", b"not found", true);
    };
    match fs::read(&path) {
        Ok(body) => {
            let send_body = method == "GET";
            respond(&mut stream, 200, content_type(&path), &body, send_body)
        }
        Err(_) => respond(&mut stream, 404, "text/plain", b"not found", true),
    }
}

/// Maps a request target onto a file inside `root`, or `None` if it escapes.
///
/// Percent-decoding happens before normalisation, so `%2e%2e` cannot smuggle a
/// parent component past the check, and every component is required to be a
/// plain name -- `..` is rejected outright rather than popped, so a traversal
/// can never resolve to a real path outside the root.
fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let decoded = percent_decode(path)?;
    let mut candidate = root.to_path_buf();
    for component in Path::new(&decoded).components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => candidate.push(part),
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    if candidate.is_dir() {
        candidate.push("index.html");
    }
    // Symlinks could still point outside the root, so confirm the real path.
    let resolved = fs::canonicalize(&candidate).ok()?;
    resolved.starts_with(root).then_some(resolved)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("csv") => "text/csv; charset=utf-8",
        Some("md" | "txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    send_body: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        405 => "Method Not Allowed",
        _ => "Not Found",
    };
    // no-store keeps a rebuilt WASM package from being served stale during
    // development, which is the failure this server exists to avoid.
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    if send_body {
        stream.write_all(body)?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_attempts_do_not_escape_the_root() {
        let root = fs::canonicalize(".").unwrap();
        for target in [
            "/../../etc/passwd",
            "/%2e%2e/%2e%2e/etc/passwd",
            "/foo/../../../etc/passwd",
            "/..%2f..%2fetc/passwd",
        ] {
            assert!(resolve(&root, target).is_none(), "escaped via {target}");
        }
    }

    #[test]
    fn query_and_fragment_are_stripped_before_lookup() {
        let root = fs::canonicalize(".").unwrap();
        let plain = resolve(&root, "/Cargo.toml");
        assert!(plain.is_some());
        assert_eq!(resolve(&root, "/Cargo.toml?v=abc123"), plain);
        assert_eq!(resolve(&root, "/Cargo.toml#top"), plain);
    }

    #[test]
    fn content_types_cover_the_browser_payloads() {
        assert_eq!(content_type(Path::new("a.wasm")), "application/wasm");
        assert_eq!(
            content_type(Path::new("a.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type(Path::new("a.bin")), "application/octet-stream");
    }
}
