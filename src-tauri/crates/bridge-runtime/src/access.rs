use std::io::{self, Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{redact_secrets, RequestSummary, MAX_REQUEST_SUMMARIES, BIND_HOST};

pub(crate) struct RecordStore {
    summaries: Vec<RequestSummary>,
    summaries_path: Option<PathBuf>,
    secrets: Vec<String>,
}

impl RecordStore {
    pub(crate) fn open(summaries_path: Option<PathBuf>) -> Self {
        let summaries = summaries_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Vec<RequestSummary>>(&text).ok())
            .map(|mut summaries| {
                if summaries.len() > MAX_REQUEST_SUMMARIES {
                    let extra = summaries.len() - MAX_REQUEST_SUMMARIES;
                    summaries.drain(..extra);
                }
                summaries
            })
            .unwrap_or_default();
        Self {
            summaries,
            summaries_path,
            secrets: Vec::new(),
        }
    }

    pub(crate) fn summaries(&self) -> Vec<RequestSummary> {
        self.summaries.clone()
    }

    pub(crate) fn set_secrets(&mut self, secrets: Vec<String>) {
        self.secrets = secrets
            .into_iter()
            .filter(|secret| !secret.is_empty())
            .collect();
    }

    pub(crate) fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.summaries_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let json = serde_json::to_string(&self.summaries).map_err(|err| err.to_string())?;
        std::fs::write(path, json).map_err(|err| err.to_string())
    }

    pub(crate) fn record(&mut self, mut summary: RequestSummary) {
        if is_health_path(&summary.path) {
            return;
        }
        let refs: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        summary.method = redact_secrets(&summary.method, &refs);
        summary.path = redact_secrets(&summary.path, &refs);
        summary.remote_addr = redact_secrets(&summary.remote_addr, &refs);
        summary.time = redact_secrets(&summary.time, &refs);
        self.summaries.push(summary);
        if self.summaries.len() > MAX_REQUEST_SUMMARIES {
            let extra = self.summaries.len() - MAX_REQUEST_SUMMARIES;
            self.summaries.drain(..extra);
        }
        let _ = self.persist();
    }

    pub(crate) fn clear(&mut self) -> Result<(), String> {
        self.summaries.clear();
        self.persist()
    }
}

fn is_health_path(path: &str) -> bool {
    path == "/health" || path == "/healthz"
}

pub(crate) fn spawn_bound_port_forwarder(
    listener: Arc<Mutex<Option<TcpListener>>>,
    sidecar_port: u16,
    store: Arc<Mutex<RecordStore>>,
    stop: Arc<AtomicBool>,
) {
    std::thread::spawn(move || loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let accepted = {
            let Ok(guard) = listener.lock() else {
                break;
            };
            let Some(listener) = guard.as_ref() else {
                break;
            };
            let _ = listener.set_nonblocking(true);
            listener.accept()
        };
        match accepted {
            Ok((stream, _)) => {
                let store = store.clone();
                std::thread::spawn(move || {
                    let _ = forward_one(stream, sidecar_port, store);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    });
}

fn forward_one(
    mut client: TcpStream,
    sidecar_port: u16,
    store: Arc<Mutex<RecordStore>>,
) -> std::io::Result<()> {
    let remote_addr = client
        .peer_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "unknown".into());
    client.set_read_timeout(Some(Duration::from_secs(300)))?;
    client.set_write_timeout(Some(Duration::from_secs(300)))?;

    let request_headers = read_headers(&mut client)?;
    let (method, path) = parse_request_line(&request_headers.text)
        .unwrap_or_else(|| ("UNKNOWN".into(), "/".into()));

    if expects_continue(&request_headers.text) {
        client.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
        client.flush()?;
    }

    let mut sidecar = TcpStream::connect((BIND_HOST, sidecar_port))?;
    sidecar.set_read_timeout(Some(Duration::from_secs(300)))?;
    sidecar.set_write_timeout(Some(Duration::from_secs(300)))?;
    let outbound = HeaderBlock {
        text: strip_header(&request_headers.text, "expect"),
        extra: request_headers.extra,
    };
    forward_message(&mut client, &mut sidecar, &outbound, false)?;

    let (mut response_headers, status) = read_final_headers(&mut sidecar)?;
    response_headers.text = force_connection_close(&response_headers.text);
    forward_message(&mut sidecar, &mut client, &response_headers, true)?;

    if let Ok(mut store) = store.lock() {
        store.record(RequestSummary {
            time: unix_millis_text(),
            method,
            status,
            remote_addr,
            path,
        });
    }
    let _ = client.shutdown(std::net::Shutdown::Both);
    Ok(())
}

struct HeaderBlock {
    text: String,
    extra: Vec<u8>,
}

fn read_headers(stream: &mut TcpStream) -> std::io::Result<HeaderBlock> {
    read_headers_from(stream, Vec::new())
}

fn read_headers_from(stream: &mut TcpStream, mut buf: Vec<u8>) -> std::io::Result<HeaderBlock> {
    let mut tmp = [0u8; 512];
    let header_end = loop {
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "eof before HTTP headers",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 64 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP headers too large",
            ));
        }
    };
    Ok(HeaderBlock {
        text: String::from_utf8_lossy(&buf[..header_end]).into_owned(),
        extra: buf[header_end..].to_vec(),
    })
}

fn forward_message<R: Read, W: Write>(
    src: &mut R,
    dst: &mut W,
    headers: &HeaderBlock,
    until_close: bool,
) -> std::io::Result<()> {
    dst.write_all(headers.text.as_bytes())?;
    let mut body = Cursor::new(headers.extra.clone()).chain(src);
    if let Some(length) = content_length(&headers.text) {
        copy_n(&mut body, dst, length)?;
    } else if is_chunked(&headers.text) {
        copy_chunked(&mut body, dst)?;
    } else if until_close {
        std::io::copy(&mut body, dst)?;
    }
    dst.flush()?;
    Ok(())
}

fn copy_n<R: Read, W: Write>(src: &mut R, dst: &mut W, mut n: usize) -> std::io::Result<()> {
    let mut buf = [0u8; 8192];
    while n > 0 {
        let want = n.min(buf.len());
        let read = src.read(&mut buf[..want])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected eof while copying body",
            ));
        }
        dst.write_all(&buf[..read])?;
        n -= read;
    }
    Ok(())
}

fn copy_chunked<R: Read, W: Write>(src: &mut R, dst: &mut W) -> std::io::Result<()> {
    loop {
        let mut size_line = Vec::new();
        read_until_crlf(src, &mut size_line)?;
        dst.write_all(&size_line)?;
        let hex = std::str::from_utf8(&size_line)
            .unwrap_or("")
            .trim()
            .split(';')
            .next()
            .unwrap_or("0");
        let size = usize::from_str_radix(hex, 16).unwrap_or(0);
        copy_n(src, dst, size)?;
        let mut crlf = [0u8; 2];
        src.read_exact(&mut crlf)?;
        dst.write_all(&crlf)?;
        dst.flush()?;
        if size == 0 {
            break;
        }
    }
    Ok(())
}

fn read_until_crlf<R: Read>(src: &mut R, out: &mut Vec<u8>) -> std::io::Result<()> {
    loop {
        let mut b = [0u8; 1];
        src.read_exact(&mut b)?;
        out.push(b[0]);
        if out.ends_with(b"\r\n") {
            return Ok(());
        }
        if out.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk size line too long",
            ));
        }
    }
}

fn is_chunked(headers: &str) -> bool {
    headers.lines().any(|line| {
        line.split_once(':')
            .map(|(name, value)| {
                name.eq_ignore_ascii_case("transfer-encoding")
                    && value.to_ascii_lowercase().contains("chunked")
            })
            .unwrap_or(false)
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_request_line(headers: &str) -> Option<(String, String)> {
    let line = headers.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;
    let path = target.split('?').next().unwrap_or(target).to_string();
    Some((method, path))
}

fn parse_status(headers: &str) -> Option<u16> {
    headers.lines().next()?.split_whitespace().nth(1)?.parse().ok()
}

fn expects_continue(headers: &str) -> bool {
    headers.lines().any(|line| {
        line.split_once(':')
            .map(|(name, value)| {
                name.eq_ignore_ascii_case("expect")
                    && value.trim().eq_ignore_ascii_case("100-continue")
            })
            .unwrap_or(false)
    })
}

fn strip_header(headers: &str, drop_name: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in headers.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once(':') {
            if name.eq_ignore_ascii_case(drop_name) {
                continue;
            }
        }
        lines.push(line.to_string());
    }
    let mut out = lines.join("\r\n");
    out.push_str("\r\n\r\n");
    out
}

fn read_final_headers(stream: &mut TcpStream) -> std::io::Result<(HeaderBlock, u16)> {
    let mut leftover = Vec::new();
    loop {
        let headers = read_headers_from(stream, leftover)?;
        let status = parse_status(&headers.text).unwrap_or(0);
        if (100..200).contains(&status) {
            leftover = headers.extra;
            continue;
        }
        return Ok((headers, status));
    }
}

fn force_connection_close(headers: &str) -> String {
    let mut saw_connection = false;
    let mut lines: Vec<String> = Vec::new();
    for line in headers.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("keep-alive") {
                continue;
            }
            if name.eq_ignore_ascii_case("connection") {
                if !saw_connection {
                    lines.push("Connection: close".into());
                    saw_connection = true;
                }
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if !saw_connection {
        lines.push("Connection: close".into());
    }
    let mut out = lines.join("\r\n");
    out.push_str("\r\n\r\n");
    out
}

fn content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}
