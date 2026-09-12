use std::fs;
use std::path::{Path, PathBuf};

use cursor2api_console_setup::{
    complete, set_autostart, status, AUTOSTART_SHORTCUT_NAME, MOVE_FOLDER_WARNING, SetupPaths,
};

fn setup_paths(tag: &str) -> SetupPaths {
    let root = std::env::temp_dir().join(format!(
        "cursor2api-setup-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    SetupPaths {
        wizard_marker: root.join("wizard.json"),
        startup_dir: root.join("Startup"),
    }
}

fn fake_exe(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let exe = dir.join("cursor2api.exe");
    fs::write(&exe, b"fake").unwrap();
    exe
}

#[test]
fn wizard_cannot_complete_without_agent_cli() {
    let paths = setup_paths("no-cli");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let err = complete(&paths, false, true, true, &exe)
        .expect_err("wizard must not finish without Agent CLI");
    assert!(
        err.contains("Agent CLI"),
        "refusal must name Agent CLI, got: {err}"
    );
    assert!(
        !paths.wizard_marker.exists(),
        "failed wizard must not write a completion marker"
    );
}

#[test]
fn wizard_cannot_complete_without_bridge_token() {
    let paths = setup_paths("no-token");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let err = complete(&paths, true, false, true, &exe)
        .expect_err("wizard must not finish without Bridge Token");
    assert!(
        err.contains("Bridge Token"),
        "refusal must name Bridge Token, got: {err}"
    );
    assert!(!paths.wizard_marker.exists());
}

#[test]
fn wizard_completes_when_agent_cli_and_bridge_token_are_present() {
    let paths = setup_paths("ok");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let view = complete(&paths, true, true, false, &exe).expect("wizard should finish");
    assert!(view.completed);
    assert!(view.can_complete);
    assert!(!view.autostart_enabled);
    assert_eq!(
        status(&paths, true, true, &exe).completed,
        true,
        "completion must persist on disk for the next Console launch"
    );
    assert_eq!(view.move_folder_warning, MOVE_FOLDER_WARNING);
}

fn shortcut_target(lnk: &Path) -> String {
    let lnk_s = lnk.to_string_lossy().replace('\'', "''");
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-STA",
            "-Command",
            &format!(
                "(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk_s}').TargetPath"
            ),
        ])
        .output()
        .expect("powershell");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn completing_wizard_with_autostart_writes_startup_shortcut_to_this_exe() {
    let paths = setup_paths("autostart-on");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let view = complete(&paths, true, true, true, &exe).expect("wizard with Autostart");
    assert!(view.autostart_enabled);
    let lnk = paths.startup_dir.join(AUTOSTART_SHORTCUT_NAME);
    assert!(lnk.is_file(), "Startup shortcut must exist at {lnk:?}");
    let target = shortcut_target(&lnk);
    let expected = exe.to_string_lossy().into_owned();
    assert!(
        target.eq_ignore_ascii_case(&expected),
        "Startup shortcut must point at this exe, got {target:?} expected {expected:?}"
    );
}

#[test]
fn disabling_autostart_removes_the_startup_shortcut() {
    let paths = setup_paths("autostart-off");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    complete(&paths, true, true, true, &exe).unwrap();
    set_autostart(&paths, false, &exe).expect("disable Autostart");
    let view = status(&paths, true, true, &exe);
    assert!(!view.autostart_enabled);
    assert!(
        !paths.startup_dir.join(AUTOSTART_SHORTCUT_NAME).exists(),
        "settings off must delete the Startup shortcut"
    );
}

#[test]
fn moving_the_exe_requires_completing_the_wizard_again() {
    let paths = setup_paths("moved-exe");
    let first = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin-a"));
    complete(&paths, true, true, false, &first).unwrap();
    assert!(status(&paths, true, true, &first).completed);
    let moved = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin-b"));
    assert!(
        !status(&paths, true, true, &moved).completed,
        "a moved Console must re-run the wizard"
    );
}
