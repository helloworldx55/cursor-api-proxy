mod access;

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use access::{spawn_bound_port_forwarder, RecordStore};

pub const DEFAULT_PREFERRED_PORT: u16 = 8765;
pub const BIND_HOST: &str = "127.0.0.1";
pub const MAX_REQUEST_SUMMARIES: usize = 200;
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub preferred_port: u16,
    pub path_env: String,
    pub sidecar_program: PathBuf,
    pub sidecar_args: Vec<String>,
    pub startup_timeout: Duration,
    pub log_path: Option<PathBuf>,
    pub summaries_path: Option<PathBuf>,
    pub max_log_bytes: u64,
    pub preferred_port_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestSummary {
    pub time: String,
    pub method: String,
    pub status: u16,
    pub remote_addr: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeState {
    Stopped,
    Running { bound_port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperatorHealth {
    AgentCliMissing,
    Stopped,
    Running { bound_port: u16 },
}

pub const AGENT_CLI_MISSING_MESSAGE: &str =
    "未在 PATH 上找到 Agent CLI（cursor-agent 或 agent），无法 Start Bridge。";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    AgentCliMissing { message: String },
    NoPortAvailable,
    SidecarFailed { message: String },
    HealthCheckTimeout,
    CursorCredentialMissing { message: String },
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
            StartError::CursorCredentialMissing { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for StartError {}

pub trait BridgeTokenStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, token: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemoryTokenStore {
    inner: Mutex<Option<String>>,
}

impl BridgeTokenStore for MemoryTokenStore {
    fn load(&self) -> Result<Option<String>, String> {
        Ok(self.inner.lock().map_err(|err| err.to_string())?.clone())
    }

    fn save(&self, token: &str) -> Result<(), String> {
        *self.inner.lock().map_err(|err| err.to_string())? = Some(token.to_string());
        Ok(())
    }
}

pub trait CursorApiKeyStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, key: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemoryCursorApiKeyStore {
    inner: Mutex<Option<String>>,
}

impl CursorApiKeyStore for MemoryCursorApiKeyStore {
    fn load(&self) -> Result<Option<String>, String> {
        Ok(self.inner.lock().map_err(|err| err.to_string())?.clone())
    }

    fn save(&self, key: &str) -> Result<(), String> {
        *self.inner.lock().map_err(|err| err.to_string())? = Some(key.to_string());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialStatus {
    pub cursor_api_key_saved: bool,
    pub agent_cli_logged_in: bool,
}

pub struct BridgeRuntime {
    config: RuntimeConfig,
    token_store: Arc<dyn BridgeTokenStore>,
    cursor_api_key_store: Arc<dyn CursorApiKeyStore>,
    child: Option<Child>,
    bound_port: Option<u16>,
    records: Arc<Mutex<RecordStore>>,
    forwarder_stop: Option<Arc<AtomicBool>>,
    bound_listener: Option<Arc<Mutex<Option<TcpListener>>>>,
    log_file: Option<Arc<Mutex<std::fs::File>>>,
}

impl BridgeRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self::with_token_store(config, Arc::new(MemoryTokenStore::default()))
    }

    pub fn with_token_store(config: RuntimeConfig, token_store: Arc<dyn BridgeTokenStore>) -> Self {
        Self::with_stores(
            config,
            token_store,
            Arc::new(MemoryCursorApiKeyStore::default()),
        )
    }

    pub fn with_stores(
        config: RuntimeConfig,
        token_store: Arc<dyn BridgeTokenStore>,
        cursor_api_key_store: Arc<dyn CursorApiKeyStore>,
    ) -> Self {
        let records = Arc::new(Mutex::new(RecordStore::open(
            config.summaries_path.clone(),
        )));
        let mut config = config;
        config.preferred_port =
            load_preferred_port(&config.preferred_port_path, config.preferred_port);
        Self {
            config,
            token_store,
            cursor_api_key_store,
            child: None,
            bound_port: None,
            records,
            forwarder_stop: None,
            bound_listener: None,
            log_file: None,
        }
    }

    pub fn request_summaries(&self) -> Vec<RequestSummary> {
        self.records
            .lock()
            .map(|store| store.summaries())
            .unwrap_or_default()
    }

    pub fn log_text(&self) -> String {
        self.config
            .log_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default()
    }

    pub fn clear_records(&self) -> Result<(), String> {
        self.records
            .lock()
            .map_err(|err| err.to_string())?
            .clear()?;
        if let Some(file) = &self.log_file {
            let mut file = file.lock().map_err(|err| err.to_string())?;
            file.set_len(0).map_err(|err| err.to_string())?;
            file.seek(SeekFrom::Start(0)).map_err(|err| err.to_string())?;
            file.flush().map_err(|err| err.to_string())?;
        } else if let Some(path) = &self.config.log_path {
            std::fs::write(path, "").map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    pub fn agent_cli_present(&self) -> bool {
        find_agent_cli(&self.config.path_env).is_some()
    }

    pub fn redetect_agent_cli(&mut self, path_env: String) -> bool {
        self.config.path_env = path_env;
        self.agent_cli_present()
    }

    pub fn operator_health(&self) -> OperatorHealth {
        match self.state() {
            BridgeState::Running { bound_port } => OperatorHealth::Running { bound_port },
            BridgeState::Stopped if !self.agent_cli_present() => OperatorHealth::AgentCliMissing,
            BridgeState::Stopped => OperatorHealth::Stopped,
        }
    }

    pub fn start_enabled(&self) -> bool {
        matches!(self.state(), BridgeState::Stopped) && self.agent_cli_present()
    }

    pub fn start_block_reason(&self) -> Option<String> {
        if self.agent_cli_present() {
            None
        } else {
            Some(AGENT_CLI_MISSING_MESSAGE.to_string())
        }
    }

    pub fn ensure_bridge_token(&self) -> Result<String, StartError> {
        let token = resolve_bridge_token(self.token_store.as_ref())?;
        persist_bridge_token(self.token_store.as_ref(), &token)?;
        Ok(token)
    }

    pub fn has_bridge_token(&self) -> Result<bool, String> {
        Ok(self
            .token_store
            .load()?
            .filter(|value| !value.is_empty())
            .is_some())
    }

    pub fn save_cursor_api_key(&self, key: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("Cursor API Key 不能为空。".into());
        }
        self.cursor_api_key_store.save(key)
    }

    pub fn credential_status(&self) -> Result<CredentialStatus, String> {
        let cursor_api_key_saved = self
            .cursor_api_key_store
            .load()?
            .filter(|value| !value.is_empty())
            .is_some();
        Ok(CredentialStatus {
            cursor_api_key_saved,
            agent_cli_logged_in: probe_agent_cli_login(&self.config.path_env),
        })
    }

    pub fn set_preferred_port(&mut self, preferred_port: u16) {
        self.config.preferred_port = preferred_port;
        persist_preferred_port(&self.config.preferred_port_path, preferred_port);
    }

    pub fn start(&mut self) -> Result<u16, StartError> {
        self.stop();

        if find_agent_cli(&self.config.path_env).is_none() {
            return Err(StartError::AgentCliMissing {
                message: AGENT_CLI_MISSING_MESSAGE.into(),
            });
        }

        let (bound_listener_socket, bound_port) = bind_loopback_from(self.config.preferred_port)
            .ok_or(StartError::NoPortAvailable)?;
        let sidecar_port = first_free_port_except(
            if bound_port < u16::MAX {
                bound_port + 1
            } else {
                1
            },
            bound_port,
        )
        .ok_or(StartError::NoPortAvailable)?;

        let token = resolve_bridge_token(self.token_store.as_ref())?;
        let cursor_api_key = load_cursor_api_key(self.cursor_api_key_store.as_ref())?;
        if cursor_api_key.is_none() && !probe_agent_cli_login(&self.config.path_env) {
            return Err(StartError::CursorCredentialMissing {
                message: "需要 Cursor API Key 或已登录的 Agent CLI，才能 Start Bridge。".into(),
            });
        }

        let mut command = Command::new(&self.config.sidecar_program);
        let capture_logs = self.config.log_path.is_some();
        command
            .args(&self.config.sidecar_args)
            .env("PATH", &self.config.path_env)
            .env("CURSOR_BRIDGE_HOST", BIND_HOST)
            .env("CURSOR_BRIDGE_PORT", sidecar_port.to_string())
            .env("CURSOR_BRIDGE_API_KEY", &token)
            .env("CURSOR_BRIDGE_CHAT_ONLY_WORKSPACE", "false")
            .env("CURSOR_BRIDGE_PROMPT_VIA_STDIN", "true")
            .stdin(Stdio::null())
            .stdout(if capture_logs {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(if capture_logs {
                Stdio::piped()
            } else {
                Stdio::null()
            });

        if let Some(key) = &cursor_api_key {
            command
                .env("CURSOR_API_KEY", key)
                .env("CURSOR_AUTH_TOKEN", key);
        } else {
            command.env_remove("CURSOR_API_KEY").env_remove("CURSOR_AUTH_TOKEN");
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|err| StartError::SidecarFailed {
            message: format!("无法拉起 Bridge sidecar：{err}"),
        })?;

        if let Some(log_path) = &self.config.log_path {
            self.log_file = attach_redacted_logs(
                &mut child,
                log_path,
                &token,
                cursor_api_key.as_deref(),
                self.config.max_log_bytes,
            );
        }

        if !wait_for_health(sidecar_port, self.config.startup_timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(StartError::HealthCheckTimeout);
        }

        if let Err(err) = persist_bridge_token(self.token_store.as_ref(), &token) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }

        if let Ok(mut store) = self.records.lock() {
            let mut secrets = vec![token.clone()];
            if let Some(key) = &cursor_api_key {
                secrets.push(key.clone());
            }
            store.set_secrets(secrets);
        }

        let stop = Arc::new(AtomicBool::new(false));
        let bound_listener = Arc::new(Mutex::new(Some(bound_listener_socket)));
        spawn_bound_port_forwarder(
            bound_listener.clone(),
            sidecar_port,
            self.records.clone(),
            stop.clone(),
        );
        self.forwarder_stop = Some(stop);
        self.bound_listener = Some(bound_listener);
        self.child = Some(child);
        self.bound_port = Some(bound_port);

        if !wait_for_health(bound_port, self.config.startup_timeout) {
            self.stop();
            return Err(StartError::HealthCheckTimeout);
        }

        Ok(bound_port)
    }

    pub fn stop(&mut self) {
        if let Some(flag) = self.forwarder_stop.take() {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(bind) = self.bound_listener.take() {
            if let Ok(mut slot) = bind.lock() {
                *slot = None;
            }
        }
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

    pub fn caller_config(&self) -> Result<String, String> {
        let bound_port = self
            .bound_port
            .ok_or_else(|| "Bridge 未启动，没有 Bound Port 可复制给 Caller。".to_string())?;
        let token = self
            .token_store
            .load()?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "尚未生成 Bridge Token。".to_string())?;
        Ok(format!(
            "Base URL: http://{BIND_HOST}:{bound_port}/v1\nBridge Token: {token}"
        ))
    }

    pub fn caller_config_display(&self) -> Result<String, String> {
        let bound_port = self
            .bound_port
            .ok_or_else(|| "Bridge 未启动，没有 Bound Port 可复制给 Caller。".to_string())?;
        Ok(format!(
            "Base URL: http://{BIND_HOST}:{bound_port}/v1\nBridge Token: （点「复制 Caller 配置」写入剪贴板，不在此窗长期显示）"
        ))
    }

    pub fn rotate_token(&mut self) -> Result<String, StartError> {
        let token = generate_bridge_token()?;
        self.token_store
            .save(&token)
            .map_err(|message| StartError::SidecarFailed { message })?;
        if self.bound_port.is_some() {
            self.start()?;
        }
        Ok(token)
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

pub fn system_path_env() -> String {
    let process = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    {
        if let Some(from_os) = windows_user_and_machine_path() {
            if process.is_empty() {
                return from_os;
            }
            return format!("{from_os};{process}");
        }
    }
    process
}

#[cfg(windows)]
fn windows_user_and_machine_path() -> Option<String> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-Command",
        "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')",
    ]);
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
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

fn resolve_bridge_token(store: &dyn BridgeTokenStore) -> Result<String, StartError> {
    if let Some(existing) = store
        .load()
        .map_err(|message| StartError::SidecarFailed { message })?
        .filter(|value| !value.is_empty())
    {
        return Ok(existing);
    }
    generate_bridge_token()
}

fn load_cursor_api_key(store: &dyn CursorApiKeyStore) -> Result<Option<String>, StartError> {
    store
        .load()
        .map(|value| value.filter(|key| !key.is_empty()))
        .map_err(|message| StartError::SidecarFailed { message })
}

fn load_preferred_port(path: &Option<PathBuf>, fallback: u16) -> u16 {
    let Some(path) = path else {
        return fallback;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return fallback;
    };
    #[derive(serde::Deserialize)]
    struct PreferredPortFile {
        preferred_port: u16,
    }
    serde_json::from_str::<PreferredPortFile>(&text)
        .ok()
        .map(|file| file.preferred_port)
        .filter(|port| *port > 0)
        .unwrap_or(fallback)
}

fn persist_preferred_port(path: &Option<PathBuf>, preferred_port: u16) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({ "preferred_port": preferred_port }))
            .unwrap_or_default(),
    );
}

fn persist_bridge_token(store: &dyn BridgeTokenStore, token: &str) -> Result<(), StartError> {
    let existing = store
        .load()
        .map_err(|message| StartError::SidecarFailed { message })?;
    if existing.as_deref() == Some(token) {
        return Ok(());
    }
    store
        .save(token)
        .map_err(|message| StartError::SidecarFailed { message })
}

fn generate_bridge_token() -> Result<String, StartError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|err| StartError::SidecarFailed {
        message: format!("无法生成 Bridge Token：{err}"),
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "***");
        }
    }
    redacted
}

fn attach_redacted_logs(
    child: &mut Child,
    log_path: &Path,
    token: &str,
    cursor_api_key: Option<&str>,
    max_log_bytes: u64,
) -> Option<Arc<Mutex<std::fs::File>>> {
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(log_path)
    else {
        return None;
    };
    let _ = file.seek(SeekFrom::End(0));
    let file = Arc::new(Mutex::new(file));
    let secrets: Vec<String> = {
        let mut values = vec![token.to_string()];
        if let Some(key) = cursor_api_key {
            values.push(key.to_string());
        }
        values
    };
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, file.clone(), secrets.clone(), max_log_bytes);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, file.clone(), secrets, max_log_bytes);
    }
    Some(file)
}

fn spawn_log_reader<R: Read + Send + 'static>(
    reader: R,
    file: Arc<Mutex<std::fs::File>>,
    secrets: Vec<String>,
    max_log_bytes: u64,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = String::new();
        while let Ok(n) = reader.read_line(&mut buf) {
            if n == 0 {
                break;
            }
            let refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
            let line = redact_secrets(&buf, &refs);
            if let Ok(mut file) = file.lock() {
                let _ = append_rolling(&mut file, line.as_bytes(), max_log_bytes);
            }
            buf.clear();
        }
    });
}

fn append_rolling(file: &mut std::fs::File, data: &[u8], max_bytes: u64) -> std::io::Result<()> {
    file.write_all(data)?;
    file.flush()?;
    if max_bytes == 0 {
        return Ok(());
    }
    let len = file.metadata()?.len();
    if len <= max_bytes {
        return Ok(());
    }
    let start = len - max_bytes;
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&tail)?;
    file.flush()?;
    Ok(())
}

fn probe_agent_cli_login(path_env: &str) -> bool {
    let Some(cli) = find_agent_cli(path_env) else {
        return false;
    };
    let mut command = Command::new(&cli);
    command.arg("status").env("PATH", path_env).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(output) = command.output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    !text.contains("not authenticated")
}

fn bind_loopback_from(preferred: u16) -> Option<(TcpListener, u16)> {
    for port in preferred..=u16::MAX {
        if let Ok(listener) = TcpListener::bind((BIND_HOST, port)) {
            return Some((listener, port));
        }
    }
    None
}

fn first_free_port_except(preferred: u16, except: u16) -> Option<u16> {
    for port in preferred..=u16::MAX {
        if port != except && TcpListener::bind((BIND_HOST, port)).is_ok() {
            return Some(port);
        }
    }
    for port in 1..preferred {
        if port != except && TcpListener::bind((BIND_HOST, port)).is_ok() {
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
