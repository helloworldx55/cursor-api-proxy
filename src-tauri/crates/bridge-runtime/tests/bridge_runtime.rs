use std::fs;
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cursor2api_bridge_runtime::{
    BridgeRuntime, BridgeState, RuntimeConfig, StartError, BIND_HOST, DEFAULT_PREFERRED_PORT,
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
    let mut stream = match TcpStream::connect((BIND_HOST, port)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {BIND_HOST}:{port}\r\nConnection: close\r\n\r\n"
    );
    use std::io::{Read, Write};
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200")
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
