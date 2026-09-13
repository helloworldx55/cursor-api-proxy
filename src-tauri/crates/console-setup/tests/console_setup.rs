use std::fs;
use std::path::{Path, PathBuf};

use cursor2api_console_setup::{
    complete, set_autostart, shell_view, status, AUTOSTART_SHORTCUT_NAME, MOVE_FOLDER_WARNING,
    SetupPaths, SetupStatus, ShellMode, SidebarItem,
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

#[test]
fn autostart_is_not_offered_before_the_wizard_completes() {
    let paths = setup_paths("no-autostart-yet");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let view = status(&paths, true, true, &exe);
    assert!(!view.completed);
    assert!(
        !view.autostart_offered,
        "Autostart must wait until the first successful wizard"
    );
    let err = set_autostart(&paths, true, &exe)
        .expect_err("settings must not enable Autostart on a broken install");
    assert!(
        err.contains("向导"),
        "refusal must mention the wizard, got: {err}"
    );
    assert!(
        !paths.startup_dir.join(AUTOSTART_SHORTCUT_NAME).exists(),
        "no Startup shortcut before wizard complete"
    );
}

#[test]
fn completed_wizard_offers_autostart() {
    let paths = setup_paths("autostart-offered");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    let view = complete(&paths, true, true, false, &exe).unwrap();
    assert!(view.autostart_offered);
    set_autostart(&paths, true, &exe).expect("settings Autostart after wizard");
    assert!(status(&paths, true, true, &exe).autostart_enabled);
}

#[test]
fn wizard_marker_stays_when_autostart_write_fails() {
    let paths = setup_paths("autostart-write-fail");
    let exe = fake_exe(&paths.wizard_marker.parent().unwrap().join("bin"));
    fs::write(&paths.startup_dir, b"not-a-directory").unwrap();
    let err = complete(&paths, true, true, true, &exe)
        .expect_err("Autostart write must fail so Console can show #wizard-error");
    assert!(
        err.contains("Autostart"),
        "failure text must reach Console naming Autostart, got: {err}"
    );
    let view = status(&paths, true, true, &exe);
    assert!(
        view.completed,
        "completion marker must not roll back when Autostart fails"
    );
    assert!(
        !view.autostart_enabled,
        "a failed Autostart write must not look enabled"
    );
}

fn setup_status(completed: bool) -> SetupStatus {
    SetupStatus {
        agent_cli_present: true,
        has_bridge_token: true,
        can_complete: true,
        completed,
        autostart_enabled: false,
        autostart_offered: completed,
        move_folder_warning: MOVE_FOLDER_WARNING.to_string(),
    }
}

#[test]
fn incomplete_wizard_is_full_window_without_sidebar() {
    let view = shell_view(
        &setup_status(false),
        Some(SidebarItem::Caller),
        Some(&()),
        false,
    );
    assert_eq!(view.mode, ShellMode::Wizard);
    assert!(
        view.nav.is_empty(),
        "incomplete wizard must not show Sidebar items"
    );
    assert_eq!(view.selected, None);
    assert_eq!(view.pane, None);
    assert!(
        !view.preferences_shows_update,
        "Release prompt must not occupy the wizard"
    );
}

#[test]
fn completed_wizard_sidebar_lists_pages_top_to_bottom() {
    let view = shell_view(&setup_status(true), None, None::<&()>, false);
    assert_eq!(view.mode, ShellMode::Sidebar);
    let labels: Vec<&str> = view.nav.iter().map(|item| item.label).collect();
    assert_eq!(
        labels,
        ["Bridge", "凭证", "Caller", "记录", "Autostart", "偏好设置"]
    );
    let ids: Vec<SidebarItem> = view.nav.iter().map(|item| item.id).collect();
    assert_eq!(
        ids,
        [
            SidebarItem::Bridge,
            SidebarItem::Credentials,
            SidebarItem::Caller,
            SidebarItem::Records,
            SidebarItem::Autostart,
            SidebarItem::Preferences,
        ]
    );
}

#[test]
fn opening_console_defaults_to_bridge() {
    let view = shell_view(&setup_status(true), None, None::<&()>, false);
    assert_eq!(view.selected, Some(SidebarItem::Bridge));
    assert_eq!(view.pane, Some(SidebarItem::Bridge));
}

#[test]
fn selecting_a_sidebar_item_shows_that_pane() {
    let view = shell_view(
        &setup_status(true),
        Some(SidebarItem::Caller),
        None::<&()>,
        false,
    );
    assert_eq!(view.selected, Some(SidebarItem::Caller));
    assert_eq!(view.pane, Some(SidebarItem::Caller));
}

#[test]
fn autostart_item_appears_only_after_the_wizard_completes() {
    let before = shell_view(&setup_status(false), None, None::<&()>, false);
    assert!(
        !before
            .nav
            .iter()
            .any(|item| item.id == SidebarItem::Autostart),
        "Autostart must not appear in Sidebar before the wizard completes"
    );
    let after = shell_view(&setup_status(true), None, None::<&()>, false);
    assert!(
        after
            .nav
            .iter()
            .any(|item| item.id == SidebarItem::Autostart),
        "Autostart must appear in Sidebar after the wizard completes"
    );
}

#[test]
fn preferences_shows_download_prompt_only_when_update_is_present_and_not_ignored() {
    let prompt = ();
    let with_prompt = shell_view(&setup_status(true), None, Some(&prompt), false);
    assert!(
        with_prompt.preferences_shows_update,
        "an unignored UpdatePrompt must show a download hint on Preferences"
    );
    let ignored = shell_view(&setup_status(true), None, Some(&prompt), true);
    assert!(
        !ignored.preferences_shows_update,
        "ignoring the UpdatePrompt this process must hide the download hint"
    );
    let none = shell_view(&setup_status(true), None, None::<&()>, false);
    assert!(
        !none.preferences_shows_update,
        "no UpdatePrompt means Preferences has no download hint"
    );
}
