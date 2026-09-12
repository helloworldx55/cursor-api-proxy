use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_PREFERRED_PORT: u16 = 8765;
pub const BIND_HOST: &str = "127.0.0.1";

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub preferred_port: u16,
    pub path_env: String,
    pub sidecar_program: PathBuf,
    pub sidecar_args: Vec<String>,
    pub startup_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeState {
    Stopped,
    Running { bound_port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    AgentCliMissing { message: String },
    NoPortAvailable,
    SidecarFailed { message: String },
    HealthCheckTimeout,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::AgentCliMissing { message } => f.write_str(message),
            StartError::NoPortAvailable => {
                write!(f, "从 Preferred Port 起没有可用端口")
            }
            StartError::SidecarFailed { message } => f.write_str(message),
            StartError::HealthCheckTimeout => {
                write!(f, "Bridge 启动后未能在超时内响应 /health")
            }
        }
    }
}

impl std::error::Error for StartError {}

pub struct BridgeRuntime {
    config: RuntimeConfig,
    child: Option<Child>,
    bound_port: Option<u16>,
}

impl BridgeRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            child: None,
            bound_port: None,
        }
    }

    pub fn set_preferred_port(&mut self, preferred_port: u16) {
        self.config.preferred_port = preferred_port;
    }

    pub fn start(&mut self) -> Result<u16, StartError> {
        self.stop();

        if find_agent_cli(&self.config.path_env).is_none() {
            return Err(StartError::AgentCliMissing {
                message: "未在 PATH 上找到 Agent CLI（cursor-agent 或 agent），无法 Start Bridge。".into(),
            });
        }

        let bound_port = first_free_port(self.config.preferred_port)
            .ok_or(StartError::NoPortAvailable)?;

        let mut command = Command::new(&self.config.sidecar_program);
        command
            .args(&self.config.sidecar_args)
            .env("PATH", &self.config.path_env)
            .env("CURSOR_BRIDGE_HOST", BIND_HOST)
            .env("CURSOR_BRIDGE_PORT", bound_port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|err| StartError::SidecarFailed {
            message: format!("无法拉起 Bridge sidecar：{err}"),
        })?;

        if !wait_for_health(bound_port, self.config.startup_timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(StartError::HealthCheckTimeout);
        }

        self.child = Some(child);
        self.bound_port = Some(bound_port);
        Ok(bound_port)
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.bound_port = None;
    }

    pub fn state(&self) -> BridgeState {
        match self.bound_port {
            Some(bound_port) => BridgeState::Running { bound_port },
            None => BridgeState::Stopped,
        }
    }

    pub fn bound_port(&self) -> Option<u16> {
        self.bound_port
    }

    pub fn preferred_port(&self) -> u16 {
        self.config.preferred_port
    }
}

impl Drop for BridgeRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn find_executable(name: &str, path_env: &str) -> Option<PathBuf> {
    find_on_path(name, path_env)
}

pub fn find_agent_cli(path_env: &str) -> Option<PathBuf> {
    for name in ["cursor-agent", "agent"] {
        if let Some(found) = find_on_path(name, path_env) {
            return Some(found);
        }
    }
    None
}

fn find_on_path(name: &str, path_env: &str) -> Option<PathBuf> {
    let extensions = path_extensions();
    for dir in split_path_env(path_env) {
        if let Some(found) = probe_dir(&dir, name, &extensions) {
            return Some(found);
        }
    }
    None
}

fn split_path_env(path_env: &str) -> Vec<PathBuf> {
    path_env
        .split(if cfg!(windows) { ';' } else { ':' })
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn path_extensions() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_string())
        .collect()
}

fn probe_dir(dir: &Path, name: &str, extensions: &[String]) -> Option<PathBuf> {
    let direct = dir.join(name);
    if file_exists(&direct) {
        return Some(direct);
    }
    for ext in extensions {
        let candidate = dir.join(format!("{name}{ext}"));
        if file_exists(&candidate) {
            return Some(candidate);
        }
        let lower = dir.join(format!("{name}{}", ext.to_lowercase()));
        if file_exists(&lower) {
            return Some(lower);
        }
    }
    None
}

fn file_exists(path: &Path) -> bool {
    path.is_file()
}

fn first_free_port(preferred: u16) -> Option<u16> {
    for port in preferred..=u16::MAX {
        if TcpListener::bind((BIND_HOST, port)).is_ok() {
            return Some(port);
        }
    }
    None
}

fn wait_for_health(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if http_ok(port, "/health") || http_ok(port, "/healthz") {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn http_ok(port: u16, path: &str) -> bool {
    let mut stream = match TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(200),
    ) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {BIND_HOST}:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200")
}
