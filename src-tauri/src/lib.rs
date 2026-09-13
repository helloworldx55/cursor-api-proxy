mod commands;

use commands::{
    bridge_log, bridge_status, caller_config, caller_config_display, clear_bridge_records,
    complete_wizard, credential_status, current_status, pick_bridge_workspace, production_config,
    redetect_agent_cli, release_check, request_summaries, rotate_bridge_token, save_cursor_api_key,
    set_autostart, set_bridge_mode, set_bridge_workspace, set_preferred_port, show_settings,
    start_bridge, start_runtime, stop_bridge, stop_runtime, wizard_status, ConsoleState,
    TrayStartItem,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (config, boot_error) = match production_config() {
        Ok(config) => (config, None),
        Err(message) => {
            eprintln!("cursor2api: {message}");
            (fallback_config(), Some(message))
        }
    };

    tauri::Builder::default()
        .manage({
            let state = ConsoleState::from_config(config);
            if let Some(message) = boot_error {
                state.set_last_error(Some(message));
            }
            state
        })
        .invoke_handler(tauri::generate_handler![
            bridge_status,
            start_bridge,
            stop_bridge,
            caller_config,
            caller_config_display,
            redetect_agent_cli,
            rotate_bridge_token,
            save_cursor_api_key,
            credential_status,
            request_summaries,
            bridge_log,
            clear_bridge_records,
            wizard_status,
            complete_wizard,
            set_autostart,
            set_preferred_port,
            set_bridge_mode,
            set_bridge_workspace,
            pick_bridge_workspace,
            release_check
        ])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "打开设置", true, None::<&str>)?;
            let start = MenuItem::with_id(
                app,
                "start",
                "启动 Bridge",
                current_status(&app.handle()).start_enabled,
                None::<&str>,
            )?;
            let stop = MenuItem::with_id(app, "stop", "停止 Bridge", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &start, &stop, &quit])?;
            app.manage(TrayStartItem(start.clone()));

            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("cursor2api — Bridge 未启动")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_settings(app),
                    "start" => {
                        let preferred = current_status(app).preferred_port;
                        let _ = start_runtime(app, preferred);
                    }
                    "stop" => {
                        let _ = stop_runtime(app);
                    }
                    "quit" => {
                        let _ = stop_runtime(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_settings(tray.app_handle());
                    }
                });

            if let Ok(icon) = commands::tray_icon_for_running(false) {
                tray = tray.icon(icon);
            } else if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let _ = stop_runtime(app);
            }
        });
}

fn fallback_config() -> cursor2api_bridge_runtime::RuntimeConfig {
    use std::time::Duration;
    cursor2api_bridge_runtime::RuntimeConfig {
        preferred_port: cursor2api_bridge_runtime::DEFAULT_PREFERRED_PORT,
        path_env: std::env::var("PATH").unwrap_or_default(),
        sidecar_program: std::path::PathBuf::from("cursor2api-missing-bridge"),
        sidecar_args: Vec::new(),
        startup_timeout: Duration::from_secs(5),
        log_path: None,
        summaries_path: None,
        max_log_bytes: cursor2api_bridge_runtime::MAX_LOG_BYTES,
        preferred_port_path: None,
        bridge_mode: cursor2api_bridge_runtime::BridgeMode::Agent,
        bridge_mode_path: None,
        bridge_workspace: std::env::temp_dir().join("cursor2api-missing-workspace"),
        default_bridge_workspace: std::env::temp_dir().join("cursor2api-missing-workspace"),
        bridge_workspace_path: None,
    }
}
