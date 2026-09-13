use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use cursor2api_bridge_runtime::{
    BridgeRuntime, BridgeState, BridgeTokenStore, CursorApiKeyStore, MemoryCursorApiKeyStore,
    MemoryTokenStore, OperatorHealth, RuntimeConfig, StartError, BIND_HOST,
    DEFAULT_PREFERRED_PORT, MAX_LOG_BYTES,
};

fn node_program() -> PathBuf {
    let output = Command::new("where.exe")
        .arg("node")
        .output()
        .expect("where.exe node");
    assert!(output.status.success(), "tests need node on PATH");
    let stdout = String::from_utf8(output.stdout).unwrap();
    PathBuf::from(stdout.lines().next().unwrap().trim())
}

fn fake_bridge_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-bridge.mjs")
}

fn isolated_path() -> String {
    std::env::temp_dir()
        .join("cursor2api-empty-path")
        .to_string_lossy()
        .into_owned()
}

fn runtime_config(path_env: String, preferred_port: u16) -> RuntimeConfig {
    RuntimeConfig {
        preferred_port,
        path_env,
        sidecar_program: node_program(),
        sidecar_args: vec![fake_bridge_script().to_string_lossy().into_owned()],
        startup_timeout: Duration::from_secs(5),
        log_path: None,
        summaries_path: None,
        max_log_bytes: MAX_LOG_BYTES,
        preferred_port_path: None,
    }
}

#[test]
fn start_is_refused_when_agent_cli_is_missing() {
    let mut runtime = BridgeRuntime::new(runtime_config(isolated_path(), DEFAULT_PREFERRED_PORT));
    let err = runtime
        .start()
        .expect_err("Start must be refused without Agent CLI");
    match err {
        StartError::AgentCliMissing { message } => {
            assert!(
                message.contains("Agent CLI"),
                "refusal must name Agent CLI, got: {message}"
            );
        }
        other => panic!("expected AgentCliMissing, got {other:?}"),
    }
    assert_eq!(runtime.state(), BridgeState::Stopped);
}

fn write_fake_agent_cli(dir: &Path, basename: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(format!("{basename}.cmd")),
        "@echo off\r\nexit /b 0\r\n",
    )
    .unwrap();
}

fn http_ok(port: u16, path: &str) -> bool {
    http_status(port, path, None) == 200
}

fn http_status(port: u16, path: &str, bearer: Option<&str>) -> u16 {
    let mut stream = match TcpStream::connect((BIND_HOST, port)) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let auth = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {BIND_HOST}:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return 0;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn lan_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

#[test]
fn start_binds_loopback_when_agent_cli_is_on_path() {
    let dir = std::env::temp_dir().join(format!("cursor2api-cli-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let preferred = 45100;
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        preferred,
    ));
    let bound = runtime.start().expect("Start with Agent CLI");
    assert_eq!(bound, preferred);
    assert!(
        http_ok(bound, "/health"),
        "Bridge must serve /health on Bound Port"
    );
    if let Some(ip) = lan_ipv4() {
        let lan = TcpStream::connect_timeout(
            &std::net::SocketAddr::from((ip, bound)),
            Duration::from_millis(400),
        );
        assert!(
            lan.is_err(),
            "Bridge must not listen on 0.0.0.0 (connect to {ip}:{bound} succeeded)"
        );
    }
    runtime.stop();
}

#[test]
fn start_uses_next_port_when_preferred_port_is_taken() {
    let dir = std::env::temp_dir().join(format!("cursor2api-port-{}", std::process::id()));
    write_fake_agent_cli(&dir, "agent");
    let preferred = 45110;
    let _hold = TcpListener::bind((BIND_HOST, preferred)).expect("hold Preferred Port");
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        preferred,
    ));
    let bound = runtime.start().expect("Start should fall back");
    assert_eq!(bound, preferred + 1);
    assert!(http_ok(bound, "/health"));
    assert!(!http_ok(preferred, "/health"));
    runtime.stop();
}

#[test]
fn dropping_runtime_releases_the_bound_port() {
    let dir = std::env::temp_dir().join(format!("cursor2api-drop-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let preferred = 45130;
    let bound = {
        let mut runtime = BridgeRuntime::new(runtime_config(
            dir.to_string_lossy().into_owned(),
            preferred,
        ));
        runtime.start().unwrap()
    };
    assert!(
        TcpListener::bind((BIND_HOST, bound)).is_ok(),
        "Bound Port must be free after Console/Runtime drop"
    );
}

#[test]
fn stop_releases_the_bound_port() {
    let dir = std::env::temp_dir().join(format!("cursor2api-stop-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let preferred = 45120;
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        preferred,
    ));
    let bound = runtime.start().unwrap();
    runtime.stop();
    assert_eq!(runtime.state(), BridgeState::Stopped);
    assert!(runtime.bound_port().is_none());
    assert!(
        TcpListener::bind((BIND_HOST, bound)).is_ok(),
        "Bound Port must be free after Stop"
    );
}

#[test]
fn caller_without_valid_bearer_is_rejected() {
    let dir = std::env::temp_dir().join(format!("cursor2api-token-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("test-bridge-token-literal").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45140),
        store,
    );
    let bound = runtime.start().expect("Start with stored Bridge Token");
    assert_eq!(http_status(bound, "/v1/models", None), 401);
    assert_eq!(http_status(bound, "/v1/models", Some("wrong-token")), 401);
    assert_eq!(
        http_status(bound, "/v1/models", Some("test-bridge-token-literal")),
        200
    );
    runtime.stop();
}

#[test]
fn first_start_generates_bridge_token_into_the_store() {
    let dir = std::env::temp_dir().join(format!("cursor2api-token-gen-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    assert!(store.load().unwrap().is_none());
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45150),
        store.clone(),
    );
    let bound = runtime.start().expect("Start should mint a Bridge Token");
    let token = store
        .load()
        .unwrap()
        .expect("Bridge Token must be saved on first ready");
    assert!(
        token.len() >= 32,
        "generated Bridge Token must be unguessable, got len {}",
        token.len()
    );
    assert_eq!(http_status(bound, "/v1/models", Some(&token)), 200);
    runtime.stop();

    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45150),
        store.clone(),
    );
    let bound = runtime.start().expect("restart with stored token");
    assert_eq!(store.load().unwrap().as_deref(), Some(token.as_str()));
    assert_eq!(http_status(bound, "/v1/models", Some(&token)), 200);
    runtime.stop();
}

#[test]
fn caller_config_uses_bound_port_and_current_token() {
    let dir = std::env::temp_dir().join(format!("cursor2api-copy-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("copy-me-bridge-token").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45160),
        store,
    );
    assert!(runtime.caller_config().is_err());
    let bound = runtime.start().unwrap();
    let copy = runtime.caller_config().expect("copy while running");
    assert!(
        copy.contains(&format!("http://127.0.0.1:{bound}/v1")),
        "copy text must use Bound Port Base URL, got: {copy}"
    );
    assert!(
        copy.contains("copy-me-bridge-token"),
        "copy text must include current Bridge Token, got: {copy}"
    );
    runtime.stop();
}

#[test]
fn rotating_bridge_token_invalidates_the_old_one() {
    let dir = std::env::temp_dir().join(format!("cursor2api-rotate-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("old-bridge-token-literal").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45170),
        store.clone(),
    );
    let bound = runtime.start().unwrap();
    assert_eq!(
        http_status(bound, "/v1/models", Some("old-bridge-token-literal")),
        200
    );
    runtime.rotate_token().expect("rotate Bridge Token");
    let bound = runtime.bound_port().expect("Bridge stays up after rotate");
    assert_eq!(
        http_status(bound, "/v1/models", Some("old-bridge-token-literal")),
        401
    );
    let new_token = store.load().unwrap().expect("rotated token saved");
    assert_ne!(new_token, "old-bridge-token-literal");
    runtime.stop();
}

#[test]
fn log_file_does_not_contain_the_bridge_token() {
    let dir = std::env::temp_dir().join(format!("cursor2api-log-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let log_path = dir.join("bridge.log");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("must-not-appear-in-logs").unwrap();
    let mut config = runtime_config(dir.to_string_lossy().into_owned(), 45180);
    config.log_path = Some(log_path.clone());
    let mut runtime = BridgeRuntime::with_token_store(config, store);
    let _bound = runtime.start().unwrap();
    let mut contents = String::new();
    for _ in 0..40 {
        contents = fs::read_to_string(&log_path).unwrap_or_default();
        if contents.contains("leaked Bridge Token") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        contents.contains("leaked Bridge Token"),
        "fixture must write a log line so redaction can be observed, got: {contents:?}"
    );
    assert!(
        !contents.contains("must-not-appear-in-logs"),
        "log file must not contain the Bridge Token, got: {contents:?}"
    );
    runtime.stop();
}

fn http_json(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect((BIND_HOST, port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: {BIND_HOST}:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

#[test]
fn start_injects_stored_cursor_api_key_into_the_sidecar() {
    let dir = std::env::temp_dir().join(format!("cursor2api-cursor-key-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let keys = Arc::new(MemoryCursorApiKeyStore::default());
    keys.save("cursor-dashboard-key-literal").unwrap();
    let mut runtime = BridgeRuntime::with_stores(
        runtime_config(dir.to_string_lossy().into_owned(), 45200),
        Arc::new(MemoryTokenStore::default()),
        keys,
    );
    let bound = runtime
        .start()
        .expect("Start with stored Cursor API Key");
    let body = http_json(bound, "/health");
    assert!(
        body.contains("\"has_cursor_api_key\":true"),
        "Bridge sidecar must receive CURSOR_API_KEY, got: {body}"
    );
    runtime.stop();
}

fn write_unauthenticated_agent_cli(dir: &Path, basename: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(format!("{basename}.cmd")),
        "@echo off\r\nif /I \"%~1\"==\"status\" (\r\n  echo Not authenticated\r\n  exit /b 1\r\n)\r\nexit /b 0\r\n",
    )
    .unwrap();
}

#[test]
fn log_file_does_not_contain_the_cursor_api_key() {
    let dir = std::env::temp_dir().join(format!("cursor2api-key-log-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let log_path = dir.join("bridge.log");
    let keys = Arc::new(MemoryCursorApiKeyStore::default());
    keys.save("must-not-appear-as-cursor-api-key").unwrap();
    let mut config = runtime_config(dir.to_string_lossy().into_owned(), 45210);
    config.log_path = Some(log_path.clone());
    let mut runtime = BridgeRuntime::with_stores(
        config,
        Arc::new(MemoryTokenStore::default()),
        keys,
    );
    let _bound = runtime.start().unwrap();
    let mut contents = String::new();
    for _ in 0..40 {
        contents = fs::read_to_string(&log_path).unwrap_or_default();
        if contents.contains("leaked Cursor API Key") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        contents.contains("leaked Cursor API Key"),
        "fixture must write a log line so redaction can be observed, got: {contents:?}"
    );
    assert!(
        !contents.contains("must-not-appear-as-cursor-api-key"),
        "log file must not contain the Cursor API Key, got: {contents:?}"
    );
    runtime.stop();
}

#[test]
fn credential_status_shows_saved_key_without_echoing_it() {
    let dir = std::env::temp_dir().join(format!("cursor2api-key-status-{}", std::process::id()));
    write_unauthenticated_agent_cli(&dir, "cursor-agent");
    let runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        45220,
    ));
    runtime
        .save_cursor_api_key("secret-cursor-api-key-literal")
        .unwrap();
    let status = runtime.credential_status().expect("credential status");
    assert!(status.cursor_api_key_saved);
    assert!(
        !status.agent_cli_logged_in,
        "unauthenticated Agent CLI must not look logged in"
    );
    assert!(
        !format!("{status:?}").contains("secret-cursor-api-key-literal"),
        "CredentialStatus must not echo the Cursor API Key"
    );
}

#[test]
fn credential_status_shows_agent_cli_login_without_requiring_a_key() {
    let dir = std::env::temp_dir().join(format!("cursor2api-login-status-{}", std::process::id()));
    write_fake_agent_cli(&dir, "agent");
    let runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        45230,
    ));
    let status = runtime.credential_status().expect("credential status");
    assert!(!status.cursor_api_key_saved);
    assert!(
        status.agent_cli_logged_in,
        "settings must show Agent CLI login as available without a pasted Cursor API Key"
    );
}

#[test]
fn start_without_cursor_api_key_when_agent_cli_is_logged_in() {
    let dir = std::env::temp_dir().join(format!("cursor2api-login-start-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        45240,
    ));
    let bound = runtime
        .start()
        .expect("Start with Agent CLI login and no Cursor API Key");
    let body = http_json(bound, "/health");
    assert!(
        body.contains("\"has_cursor_api_key\":false"),
        "default config must not inject a Cursor API Key, got: {body}"
    );
    runtime.stop();
}

#[test]
fn start_is_refused_without_cursor_api_key_or_agent_cli_login() {
    let dir = std::env::temp_dir().join(format!("cursor2api-no-cred-{}", std::process::id()));
    write_unauthenticated_agent_cli(&dir, "cursor-agent");
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        45250,
    ));
    let err = runtime
        .start()
        .expect_err("Start must be refused without Cursor API Key or Agent CLI login");
    match err {
        StartError::CursorCredentialMissing { message } => {
            assert!(
                message.contains("Cursor API Key"),
                "refusal must name Cursor API Key, got: {message}"
            );
            assert!(
                message.contains("Agent CLI"),
                "refusal must name Agent CLI, got: {message}"
            );
        }
        other => panic!("expected CursorCredentialMissing, got {other:?}"),
    }
    assert_eq!(runtime.state(), BridgeState::Stopped);
}

fn http_post(port: u16, path: &str, bearer: &str, body: &str) -> u16 {
    let mut stream = match TcpStream::connect((BIND_HOST, port)) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {BIND_HOST}:{port}\r\nAuthorization: Bearer {bearer}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return 0;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

#[test]
fn finished_caller_request_is_kept_as_request_summary() {
    let dir = std::env::temp_dir().join(format!("cursor2api-summary-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("summary-bridge-token").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45260),
        store,
    );
    let bound = runtime.start().expect("Start Bridge");
    assert_eq!(
        http_status(bound, "/v1/models", Some("summary-bridge-token")),
        200
    );
    let summaries = runtime.request_summaries();
    assert_eq!(
        summaries.len(),
        1,
        "one finished Caller request, got {summaries:?}"
    );
    let summary = &summaries[0];
    assert_eq!(summary.method, "GET");
    assert_eq!(summary.status, 200);
    assert_eq!(summary.path, "/v1/models");
    assert!(
        summary.remote_addr.contains("127.0.0.1"),
        "remote_addr must be loopback, got {}",
        summary.remote_addr
    );
    assert!(
        !summary.time.is_empty(),
        "Request Summary must include time"
    );
    runtime.stop();
}

#[test]
fn request_summaries_keep_only_the_latest_200() {
    let dir = std::env::temp_dir().join(format!("cursor2api-summary-cap-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("cap-bridge-token").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 46100),
        store,
    );
    let bound = runtime.start().unwrap();
    for i in 0..201 {
        let path = format!("/v1/n/{i}");
        assert_eq!(http_status(bound, &path, Some("cap-bridge-token")), 200);
    }
    std::thread::sleep(Duration::from_millis(50));
    let summaries = runtime.request_summaries();
    assert_eq!(summaries.len(), 200);
    assert_eq!(summaries[0].path, "/v1/n/1");
    assert_eq!(summaries[199].path, "/v1/n/200");
    runtime.stop();
}

#[test]
fn request_summary_does_not_contain_bridge_token_or_request_body() {
    let dir = std::env::temp_dir().join(format!("cursor2api-summary-redact-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("must-not-appear-in-summary").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45280),
        store,
    );
    let bound = runtime.start().unwrap();
    assert_eq!(
        http_post(
            bound,
            "/v1/models",
            "must-not-appear-in-summary",
            "{\"prompt\":\"secret-request-body-literal\"}",
        ),
        200
    );
    let summaries = runtime.request_summaries();
    assert_eq!(summaries.len(), 1);
    let dumped = format!("{:?}", summaries);
    assert!(
        !dumped.contains("must-not-appear-in-summary"),
        "Request Summary must not contain the Bridge Token, got {dumped}"
    );
    assert!(
        !dumped.contains("secret-request-body-literal"),
        "Request Summary must not contain the message body, got {dumped}"
    );
    runtime.stop();
}

#[test]
fn request_summary_redacts_cursor_api_key_from_path() {
    let dir = std::env::temp_dir().join(format!("cursor2api-summary-key-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let keys = Arc::new(MemoryCursorApiKeyStore::default());
    keys.save("must-not-appear-as-cursor-api-key").unwrap();
    let store = Arc::new(MemoryTokenStore::default());
    store.save("path-redact-token").unwrap();
    let mut runtime = BridgeRuntime::with_stores(
        runtime_config(dir.to_string_lossy().into_owned(), 45290),
        store,
        keys,
    );
    let bound = runtime.start().unwrap();
    assert_eq!(
        http_status(
            bound,
            "/v1/must-not-appear-as-cursor-api-key",
            Some("path-redact-token"),
        ),
        200
    );
    let dumped = format!("{:?}", runtime.request_summaries());
    assert!(
        !dumped.contains("must-not-appear-as-cursor-api-key"),
        "Request Summary must not contain the Cursor API Key, got {dumped}"
    );
    runtime.stop();
}

#[test]
fn log_file_rolls_when_it_exceeds_the_cap() {
    let dir = std::env::temp_dir().join(format!("cursor2api-log-roll-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    fs::create_dir_all(&dir).unwrap();
    let log_path = dir.join("bridge.log");
    let mut config = runtime_config(dir.to_string_lossy().into_owned(), 45300);
    config.log_path = Some(log_path.clone());
    config.max_log_bytes = 64;
    config.sidecar_args.push("400".into());
    let mut runtime = BridgeRuntime::new(config);
    let _bound = runtime.start().unwrap();
    let mut len = 0u64;
    let mut contents = String::new();
    for _ in 0..40 {
        contents = fs::read_to_string(&log_path).unwrap_or_default();
        len = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
        if contents.contains("LOG_END_MARKER") || len > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    for _ in 0..40 {
        len = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
        contents = fs::read_to_string(&log_path).unwrap_or_default();
        if len <= 64 && contents.contains("LOG_END_MARKER") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        len <= 64,
        "rolling log must stay within the cap, got {len} bytes: {contents:?}"
    );
    assert!(
        contents.contains("LOG_END_MARKER"),
        "rolling log must keep the newest bytes, got {contents:?}"
    );
    runtime.stop();
}

#[test]
fn clear_records_wipes_summaries_and_logs_in_memory_and_on_disk() {
    let dir = std::env::temp_dir().join(format!("cursor2api-clear-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    fs::create_dir_all(&dir).unwrap();
    let log_path = dir.join("bridge.log");
    let summaries_path = dir.join("request-summaries.json");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("clear-bridge-token").unwrap();
    let mut config = runtime_config(dir.to_string_lossy().into_owned(), 45310);
    config.log_path = Some(log_path.clone());
    config.summaries_path = Some(summaries_path.clone());
    let mut runtime = BridgeRuntime::with_token_store(config, store);
    let bound = runtime.start().unwrap();
    assert_eq!(
        http_status(bound, "/v1/models", Some("clear-bridge-token")),
        200
    );
    let mut log_ready = String::new();
    for _ in 0..40 {
        log_ready = fs::read_to_string(&log_path).unwrap_or_default();
        if !log_ready.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !runtime.request_summaries().is_empty(),
        "need a Request Summary before clear"
    );
    assert!(
        summaries_path.is_file(),
        "Request Summary must be written to App Data"
    );
    assert!(
        !log_ready.is_empty(),
        "log file must have content before clear"
    );

    runtime.clear_records().expect("clear records");
    assert!(runtime.request_summaries().is_empty());
    assert_eq!(fs::read_to_string(&log_path).unwrap_or_default(), "");
    let persisted = fs::read_to_string(&summaries_path).unwrap_or_default();
    assert!(
        persisted.trim().is_empty() || persisted.trim() == "[]",
        "disk Request Summary must be empty after clear, got {persisted:?}"
    );

    let restarted = BridgeRuntime::with_token_store(
        {
            let mut config = runtime_config(dir.to_string_lossy().into_owned(), 45311);
            config.log_path = Some(log_path.clone());
            config.summaries_path = Some(summaries_path.clone());
            config
        },
        Arc::new(MemoryTokenStore::default()),
    );
    assert!(
        restarted.request_summaries().is_empty(),
        "cleared Request Summary must not reload from disk"
    );
    runtime.stop();
}

#[test]
fn health_distinguishes_missing_agent_cli_from_stopped() {
    let missing = BridgeRuntime::new(runtime_config(isolated_path(), DEFAULT_PREFERRED_PORT));
    assert_eq!(missing.operator_health(), OperatorHealth::AgentCliMissing);
    assert!(!missing.start_enabled());
    assert!(
        missing
            .start_block_reason()
            .expect("block copy")
            .contains("Agent CLI")
    );

    let dir = std::env::temp_dir().join(format!("cursor2api-health-cli-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let present = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        DEFAULT_PREFERRED_PORT,
    ));
    assert_eq!(present.operator_health(), OperatorHealth::Stopped);
    assert!(present.start_enabled());
    assert!(present.start_block_reason().is_none());
}

#[test]
fn health_is_running_only_while_bridge_is_up() {
    let dir = std::env::temp_dir().join(format!("cursor2api-health-run-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let mut runtime = BridgeRuntime::new(runtime_config(
        dir.to_string_lossy().into_owned(),
        45400,
    ));
    let bound = runtime.start().unwrap();
    assert_eq!(
        runtime.operator_health(),
        OperatorHealth::Running { bound_port: bound }
    );
    assert!(!runtime.start_enabled());
    runtime.stop();
    assert_eq!(runtime.operator_health(), OperatorHealth::Stopped);
    assert!(runtime.start_enabled());
}

#[test]
fn ensure_bridge_token_mints_without_starting_the_bridge() {
    let dir = std::env::temp_dir().join(format!("cursor2api-ensure-token-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    let runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45410),
        store.clone(),
    );
    assert!(!runtime.has_bridge_token().unwrap());
    let token = runtime
        .ensure_bridge_token()
        .expect("first successful setup must mint a Bridge Token");
    assert!(token.len() >= 32);
    assert_eq!(store.load().unwrap().as_deref(), Some(token.as_str()));
    assert_eq!(runtime.state(), BridgeState::Stopped);
    assert!(
        TcpListener::bind((BIND_HOST, 45410)).is_ok(),
        "minting a Bridge Token must not bind the Preferred Port"
    );
}

#[test]
fn preferred_port_is_reread_from_app_data_by_a_new_runtime() {
    let dir = std::env::temp_dir().join(format!("cursor2api-port-file-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    write_fake_agent_cli(&dir, "cursor-agent");
    let port_path = dir.join("preferred-port.json");
    let mut first = BridgeRuntime::new({
        let mut config = runtime_config(dir.to_string_lossy().into_owned(), DEFAULT_PREFERRED_PORT);
        config.preferred_port_path = Some(port_path.clone());
        config
    });
    first.set_preferred_port(8777);
    assert_eq!(first.preferred_port(), 8777);
    drop(first);

    let second = BridgeRuntime::new({
        let mut config = runtime_config(dir.to_string_lossy().into_owned(), DEFAULT_PREFERRED_PORT);
        config.preferred_port_path = Some(port_path);
        config
    });
    assert_eq!(
        second.preferred_port(),
        8777,
        "Preferred Port must survive a new zip folder via App Data"
    );
}

struct FailingSaveStore;

impl BridgeTokenStore for FailingSaveStore {
    fn load(&self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn save(&self, _token: &str) -> Result<(), String> {
        Err("凭据库写入失败".into())
    }
}

#[test]
fn failed_start_does_not_leave_an_orphaned_sidecar() {
    let dir = std::env::temp_dir().join(format!("cursor2api-orphan-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let preferred = 45420;
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), preferred),
        Arc::new(FailingSaveStore),
    );
    assert!(runtime.start().is_err());
    assert_eq!(runtime.state(), BridgeState::Stopped);
    for port in preferred..=preferred + 3 {
        assert!(
            !http_ok(port, "/health"),
            "failed Start must not leave a sidecar on {port}"
        );
        assert!(
            TcpListener::bind((BIND_HOST, port)).is_ok(),
            "failed Start must release {port}"
        );
    }
}

#[test]
fn caller_display_omits_the_bridge_token() {
    let dir = std::env::temp_dir().join(format!("cursor2api-display-{}", std::process::id()));
    write_fake_agent_cli(&dir, "cursor-agent");
    let store = Arc::new(MemoryTokenStore::default());
    store.save("must-not-stay-on-screen").unwrap();
    let mut runtime = BridgeRuntime::with_token_store(
        runtime_config(dir.to_string_lossy().into_owned(), 45430),
        store,
    );
    let bound = runtime.start().unwrap();
    let copy = runtime.caller_config().unwrap();
    assert!(copy.contains("must-not-stay-on-screen"));
    let display = runtime
        .caller_config_display()
        .expect("settings can show a Bound Port hint");
    assert!(
        display.contains(&format!("http://127.0.0.1:{bound}/v1")),
        "display must still show Base URL with Bound Port, got: {display}"
    );
    assert!(
        !display.contains("must-not-stay-on-screen"),
        "settings must not keep the Bridge Token on screen, got: {display}"
    );
    runtime.stop();
}

#[test]
fn redetect_agent_cli_picks_up_a_new_path() {
    let dir = std::env::temp_dir().join(format!("cursor2api-redetect-{}", std::process::id()));
    let mut runtime = BridgeRuntime::new(runtime_config(isolated_path(), DEFAULT_PREFERRED_PORT));
    assert!(!runtime.agent_cli_present());
    write_fake_agent_cli(&dir, "cursor-agent");
    assert!(
        runtime.redetect_agent_cli(dir.to_string_lossy().into_owned()),
        "re-detect must find Agent CLI after it appears on PATH"
    );
    assert!(runtime.agent_cli_present());
    assert_eq!(runtime.operator_health(), OperatorHealth::Stopped);
}
