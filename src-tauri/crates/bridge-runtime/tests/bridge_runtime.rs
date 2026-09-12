use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use cursor2api_bridge_runtime::{
    BridgeRuntime, BridgeState, BridgeTokenStore, CursorApiKeyStore, MemoryCursorApiKeyStore,
    MemoryTokenStore, RuntimeConfig, StartError, BIND_HOST, DEFAULT_PREFERRED_PORT,
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
