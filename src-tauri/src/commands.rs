use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cursor2api_bridge_runtime::{
    find_executable, BridgeRuntime, BridgeState, BridgeTokenStore, CursorApiKeyStore,
    RuntimeConfig, DEFAULT_PREFERRED_PORT,
};
use cursor2api_console_setup::{self as setup, SetupPaths, SetupStatus};
use cursor2api_release_check::{
    check_for_update, GitHubReleaseSource, UpdatePrompt,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct ConsoleState {
    runtime: Mutex<BridgeRuntime>,
    last_error: Mutex<Option<String>>,
    setup: SetupPaths,
}

#[derive(Clone, Serialize)]
pub struct BridgeStatusView {
    pub running: bool,
    pub bound_port: Option<u16>,
    pub preferred_port: u16,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct CredentialStatusView {
    pub cursor_api_key_saved: bool,
    pub agent_cli_logged_in: bool,
}

impl ConsoleState {
    pub fn from_config(config: RuntimeConfig) -> Self {
        Self {
            runtime: Mutex::new(BridgeRuntime::with_stores(
                config,
                Arc::new(KeyringTokenStore),
                Arc::new(KeyringCursorApiKeyStore),
            )),
            last_error: Mutex::new(None),
            setup: setup::production_paths(app_data_dir()),
        }
    }

    pub fn set_last_error(&self, message: Option<String>) {
        *self.last_error.lock().expect("error lock") = message;
    }

    fn snapshot(&self) -> BridgeStatusView {
        let runtime = self.runtime.lock().expect("runtime lock");
        let error = self.last_error.lock().expect("error lock").clone();
        match runtime.state() {
            BridgeState::Running { bound_port } => BridgeStatusView {
                running: true,
                bound_port: Some(bound_port),
                preferred_port: runtime.preferred_port(),
                error,
            },
            BridgeState::Stopped => BridgeStatusView {
                running: false,
                bound_port: None,
                preferred_port: runtime.preferred_port(),
                error,
            },
        }
    }
}

pub fn production_config() -> Result<RuntimeConfig, String> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let sidecar_program = find_executable("node", &path_env).ok_or_else(|| {
        "未找到 Node。v0 开发态使用系统 Node 拉起 npm 包 cursor-api-proxy。".to_string()
    })?;
    let cli = resolve_bridge_cli()?;
    Ok(RuntimeConfig {
        preferred_port: DEFAULT_PREFERRED_PORT,
        path_env,
        sidecar_program,
        sidecar_args: vec![cli.to_string_lossy().into_owned()],
        startup_timeout: Duration::from_secs(20),
        log_path: Some(app_data_dir().join("bridge.log")),
        summaries_path: Some(app_data_dir().join("request-summaries.json")),
        max_log_bytes: cursor2api_bridge_runtime::MAX_LOG_BYTES,
    })
}

fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("cursor2api");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn resolve_bridge_cli() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("node_modules/cursor-api-proxy/dist/cli.js"));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("node_modules/cursor-api-proxy/dist/cli.js"),
    );
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("node_modules/cursor-api-proxy/dist/cli.js"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("node_modules/cursor-api-proxy/dist/cli.js"));
            }
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "未找到 cursor-api-proxy（期望 node_modules/cursor-api-proxy/dist/cli.js）。".into()
        })
}

pub struct KeyringTokenStore;

impl BridgeTokenStore for KeyringTokenStore {
    fn load(&self) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new("cursor2api", "bridge-token")
            .map_err(|err| err.to_string())?;
        match entry.get_password() {
            Ok(token) if !token.is_empty() => Ok(Some(token)),
            Ok(_) => Ok(None),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn save(&self, token: &str) -> Result<(), String> {
        let entry = keyring::Entry::new("cursor2api", "bridge-token")
            .map_err(|err| err.to_string())?;
        entry.set_password(token).map_err(|err| err.to_string())
    }
}

pub struct KeyringCursorApiKeyStore;

impl CursorApiKeyStore for KeyringCursorApiKeyStore {
    fn load(&self) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new("cursor2api", "cursor-api-key")
            .map_err(|err| err.to_string())?;
        match entry.get_password() {
            Ok(key) if !key.is_empty() => Ok(Some(key)),
            Ok(_) => Ok(None),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn save(&self, key: &str) -> Result<(), String> {
        let entry = keyring::Entry::new("cursor2api", "cursor-api-key")
            .map_err(|err| err.to_string())?;
        entry.set_password(key).map_err(|err| err.to_string())
    }
}

pub fn start_runtime(app: &AppHandle, preferred_port: u16) -> Result<BridgeStatusView, String> {
    let state = app.state::<ConsoleState>();
    let result = {
        let mut runtime = state.runtime.lock().map_err(|_| "runtime lock")?;
        runtime.set_preferred_port(preferred_port);
        runtime.start().map_err(|err| err.to_string())
    };
    match result {
        Ok(_) => {
            *state.last_error.lock().map_err(|_| "error lock")? = None;
            let view = state.snapshot();
            publish_status(app, &view)?;
            Ok(view)
        }
        Err(message) => {
            *state.last_error.lock().map_err(|_| "error lock")? = Some(message.clone());
            let view = state.snapshot();
            let _ = publish_status(app, &view);
            Err(message)
        }
    }
}

pub fn stop_runtime(app: &AppHandle) -> Result<BridgeStatusView, String> {
    let state = app.state::<ConsoleState>();
    {
        let mut runtime = state.runtime.lock().map_err(|_| "runtime lock")?;
        runtime.stop();
    }
    *state.last_error.lock().map_err(|_| "error lock")? = None;
    let view = state.snapshot();
    publish_status(app, &view)?;
    Ok(view)
}

pub fn current_status(app: &AppHandle) -> BridgeStatusView {
    app.state::<ConsoleState>().snapshot()
}

fn publish_status(app: &AppHandle, view: &BridgeStatusView) -> Result<(), String> {
    let tooltip = if view.running {
        format!(
            "cursor2api — Bridge 运行中 · Bound Port {}",
            view.bound_port.unwrap_or(0)
        )
    } else {
        "cursor2api — Bridge 未启动".into()
    };
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&tooltip));
    }
    app.emit("bridge-status", view)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn bridge_status(state: State<ConsoleState>) -> BridgeStatusView {
    state.snapshot()
}

#[tauri::command]
pub fn start_bridge(app: AppHandle, preferred_port: Option<u16>) -> Result<BridgeStatusView, String> {
    start_runtime(&app, preferred_port.unwrap_or(DEFAULT_PREFERRED_PORT))
}

#[tauri::command]
pub fn stop_bridge(app: AppHandle) -> Result<BridgeStatusView, String> {
    stop_runtime(&app)
}

#[tauri::command]
pub fn caller_config(state: State<ConsoleState>) -> Result<String, String> {
    state
        .runtime
        .lock()
        .map_err(|_| "runtime lock".to_string())?
        .caller_config()
}

#[tauri::command]
pub fn rotate_bridge_token(app: AppHandle) -> Result<BridgeStatusView, String> {
    let state = app.state::<ConsoleState>();
    let result = {
        let mut runtime = state.runtime.lock().map_err(|_| "runtime lock")?;
        runtime.rotate_token().map_err(|err| err.to_string())
    };
    match result {
        Ok(_) => {
            *state.last_error.lock().map_err(|_| "error lock")? = None;
            let view = state.snapshot();
            publish_status(&app, &view)?;
            Ok(view)
        }
        Err(message) => {
            *state.last_error.lock().map_err(|_| "error lock")? = Some(message.clone());
            let view = state.snapshot();
            let _ = publish_status(&app, &view);
            Err(message)
        }
    }
}

#[tauri::command]
pub fn save_cursor_api_key(
    state: State<ConsoleState>,
    key: String,
) -> Result<CredentialStatusView, String> {
    let runtime = state.runtime.lock().map_err(|_| "runtime lock".to_string())?;
    runtime.save_cursor_api_key(&key)?;
    let status = runtime.credential_status()?;
    Ok(CredentialStatusView {
        cursor_api_key_saved: status.cursor_api_key_saved,
        agent_cli_logged_in: status.agent_cli_logged_in,
    })
}

#[tauri::command]
pub fn credential_status(state: State<ConsoleState>) -> Result<CredentialStatusView, String> {
    let status = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock".to_string())?
        .credential_status()?;
    Ok(CredentialStatusView {
        cursor_api_key_saved: status.cursor_api_key_saved,
        agent_cli_logged_in: status.agent_cli_logged_in,
    })
}

#[tauri::command]
pub fn request_summaries(state: State<ConsoleState>) -> Vec<cursor2api_bridge_runtime::RequestSummary> {
    state
        .runtime
        .lock()
        .map(|runtime| runtime.request_summaries())
        .unwrap_or_default()
}

#[tauri::command]
pub fn bridge_log(state: State<ConsoleState>) -> String {
    state
        .runtime
        .lock()
        .map(|runtime| runtime.log_text())
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_bridge_records(state: State<ConsoleState>) -> Result<(), String> {
    state
        .runtime
        .lock()
        .map_err(|_| "runtime lock".to_string())?
        .clear_records()
}

fn setup_view(state: &ConsoleState) -> Result<SetupStatus, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock".to_string())?;
    let exe = current_exe()?;
    Ok(setup::status(
        &state.setup,
        runtime.agent_cli_present(),
        runtime.has_bridge_token()?,
        &exe,
    ))
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn wizard_status(state: State<ConsoleState>) -> Result<SetupStatus, String> {
    setup_view(&state)
}

#[tauri::command]
pub fn complete_wizard(
    state: State<ConsoleState>,
    enable_autostart: bool,
) -> Result<SetupStatus, String> {
    let (agent_cli_present, has_bridge_token) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock".to_string())?;
        (runtime.agent_cli_present(), runtime.has_bridge_token()?)
    };
    setup::complete(
        &state.setup,
        agent_cli_present,
        has_bridge_token,
        enable_autostart,
        &current_exe()?,
    )
}

#[tauri::command]
pub fn set_autostart(state: State<ConsoleState>, enabled: bool) -> Result<SetupStatus, String> {
    setup::set_autostart(&state.setup, enabled, &current_exe()?)?;
    setup_view(&state)
}

#[tauri::command]
pub fn release_check() -> Result<Option<UpdatePrompt>, String> {
    match check_for_update(
        env!("CARGO_PKG_VERSION"),
        &GitHubReleaseSource::default(),
    ) {
        Ok(prompt) => Ok(prompt),
        Err(_) => Ok(None),
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
