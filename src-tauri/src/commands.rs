use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cursor2api_bridge_runtime::{
    find_executable, BridgeRuntime, BridgeState, BridgeTokenStore, RuntimeConfig,
    DEFAULT_PREFERRED_PORT,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct ConsoleState {
    runtime: Mutex<BridgeRuntime>,
    last_error: Mutex<Option<String>>,
}

#[derive(Clone, Serialize)]
pub struct BridgeStatusView {
    pub running: bool,
    pub bound_port: Option<u16>,
    pub preferred_port: u16,
    pub error: Option<String>,
}

impl ConsoleState {
    pub fn from_config(config: RuntimeConfig) -> Self {
        Self {
            runtime: Mutex::new(BridgeRuntime::with_token_store(
                config,
                Arc::new(KeyringTokenStore),
            )),
            last_error: Mutex::new(None),
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
        log_path: None,
    })
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

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
