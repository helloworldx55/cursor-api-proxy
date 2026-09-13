use std::path::{Path, PathBuf};

pub const AUTOSTART_SHORTCUT_NAME: &str = "cursor2api.lnk";

pub const MOVE_FOLDER_WARNING: &str =
    "移动解压目录会弄坏 Autostart，需重新完成向导。";

#[derive(Debug, Clone)]
pub struct SetupPaths {
    pub wizard_marker: PathBuf,
    pub startup_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SetupStatus {
    pub agent_cli_present: bool,
    pub has_bridge_token: bool,
    pub can_complete: bool,
    pub completed: bool,
    pub autostart_enabled: bool,
    pub autostart_offered: bool,
    pub move_folder_warning: String,
}

pub fn status(
    paths: &SetupPaths,
    agent_cli_present: bool,
    has_bridge_token: bool,
    exe: &Path,
) -> SetupStatus {
    let completed = wizard_completed(paths, exe);
    SetupStatus {
        agent_cli_present,
        has_bridge_token,
        can_complete: agent_cli_present && has_bridge_token,
        completed,
        autostart_enabled: autostart_shortcut(paths).is_file(),
        autostart_offered: completed,
        move_folder_warning: MOVE_FOLDER_WARNING.to_string(),
    }
}

pub fn complete(
    paths: &SetupPaths,
    agent_cli_present: bool,
    has_bridge_token: bool,
    enable_autostart: bool,
    exe: &Path,
) -> Result<SetupStatus, String> {
    if !agent_cli_present {
        return Err("未在 PATH 上找到 Agent CLI，不能完成首次向导。".into());
    }
    if !has_bridge_token {
        return Err("尚无 Bridge Token，不能完成首次向导。".into());
    }
    write_wizard_marker(paths, exe)?;
    set_autostart(paths, enable_autostart, exe)?;
    Ok(status(paths, agent_cli_present, has_bridge_token, exe))
}

pub fn set_autostart(paths: &SetupPaths, enabled: bool, exe: &Path) -> Result<(), String> {
    if enabled && !wizard_completed(paths, exe) {
        return Err("首次向导完成前不能启用 Autostart。".into());
    }
    let lnk = autostart_shortcut(paths);
    if enabled {
        std::fs::create_dir_all(&paths.startup_dir).map_err(|err| err.to_string())?;
        write_startup_shortcut(&lnk, exe)?;
    } else if lnk.exists() {
        std::fs::remove_file(&lnk).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn write_startup_shortcut(lnk: &Path, exe: &Path) -> Result<(), String> {
    let lnk_s = powershell_literal(lnk);
    let exe_s = powershell_literal(exe);
    let work_s = exe
        .parent()
        .map(powershell_literal)
        .unwrap_or_default();
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({lnk_s}); $s.TargetPath = {exe_s}; $s.WorkingDirectory = {work_s}; $s.Save()"
    );
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-STA", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|err| format!("无法写入 Autostart 快捷方式：{err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("无法写入 Autostart 快捷方式：{stderr}"));
    }
    if !lnk.is_file() {
        return Err("Autostart 快捷方式未写入 Startup。".into());
    }
    Ok(())
}

fn powershell_literal(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\'', "''");
    format!("'{raw}'")
}

fn write_wizard_marker(paths: &SetupPaths, exe: &Path) -> Result<(), String> {
    if let Some(parent) = paths.wizard_marker.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string(&WizardMarker {
        completed: true,
        exe: exe.to_string_lossy().into_owned(),
    })
    .map_err(|err| err.to_string())?;
    std::fs::write(&paths.wizard_marker, json).map_err(|err| err.to_string())
}

fn wizard_completed(paths: &SetupPaths, exe: &Path) -> bool {
    let Some(marker) = std::fs::read_to_string(&paths.wizard_marker)
        .ok()
        .and_then(|text| serde_json::from_str::<WizardMarker>(&text).ok())
    else {
        return false;
    };
    marker.completed && same_exe(&marker.exe, exe)
}

fn same_exe(recorded: &str, exe: &Path) -> bool {
    let recorded = Path::new(recorded);
    if recorded == exe {
        return true;
    }
    match (std::fs::canonicalize(recorded), std::fs::canonicalize(exe)) {
        (Ok(left), Ok(right)) => left == right,
        _ => recorded.to_string_lossy().eq_ignore_ascii_case(&exe.to_string_lossy()),
    }
}

pub fn production_paths(app_data: PathBuf) -> SetupPaths {
    SetupPaths {
        wizard_marker: app_data.join("wizard.json"),
        startup_dir: windows_startup_dir(),
    }
}

pub fn windows_startup_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
}

fn autostart_shortcut(paths: &SetupPaths) -> PathBuf {
    paths.startup_dir.join(AUTOSTART_SHORTCUT_NAME)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WizardMarker {
    completed: bool,
    #[serde(default)]
    exe: String,
}
