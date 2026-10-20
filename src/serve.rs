//! Minimal dependency-free static file server used for local development.
//!
//! This is a development convenience only — the published site is fully
//! static and is served by Vercel's CDN.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn serve(dir: &Path, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    tracing::info!("serving {} on http://localhost:{port}", dir.display());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let root: PathBuf = dir.to_path_buf();
                std::thread::spawn(move || {
                    if let Err(err) = handle(stream, &root) {
                        tracing::debug!("connection error: {err:#}");
                    }
                });
            }
            Err(err) => tracing::warn!("accept failed: {err}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, root: &Path) -> Result<()> {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut request_line)?;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let requested = target.split(['?', '#']).next().unwrap_or("/");
    let rel = percent_decode(requested.trim_start_matches('/'));

    let (status, body, content_type) = if rel.split('/').any(|segment| segment == "..") {
        (
            "403 Forbidden",
            b"Forbidden".to_vec(),
            "text/plain; charset=utf-8",
        )
    } else if !matches!(method, "GET" | "HEAD") {
        (
            "405 Method Not Allowed",
            b"Method not allowed".to_vec(),
            "text/plain; charset=utf-8",
        )
    } else {
        let mut file = root.join(&rel);
        if file.is_dir() {
            file = file.join("index.html");
        }
        if file.is_file() {
            match std::fs::read(&file) {
                Ok(bytes) => ("200 OK", bytes, content_type(&file)),
                Err(_) => (
                    "500 Internal Server Error",
                    b"Read error".to_vec(),
                    "text/plain; charset=utf-8",
                ),
            }
        } else {
            let not_found = root.join("404.html");
            if not_found.is_file() {
                let bytes = std::fs::read(&not_found).unwrap_or_else(|_| b"404".to_vec());
                ("404 Not Found", bytes, "text/html; charset=utf-8")
            } else {
                (
                    "404 Not Found",
                    b"404 Not Found".to_vec(),
                    "text/plain; charset=utf-8",
                )
            }
        }
    };

    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(&body)?;
    }
    stream.flush()?;
    Ok(())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"),
                16,
            ) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
