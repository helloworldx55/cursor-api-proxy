mod commands;

use commands::{
    bridge_status, caller_config, credential_status, current_status, production_config,
    rotate_bridge_token, save_cursor_api_key, show_settings, start_bridge, start_runtime,
    stop_bridge, stop_runtime, ConsoleState,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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
            rotate_bridge_token,
            save_cursor_api_key,
            credential_status
        ])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "打开设置", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "启动 Bridge", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "停止 Bridge", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &start, &stop, &quit])?;

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

            if let Some(icon) = app.default_window_icon() {
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
    }
}
