use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use cursor2api_release_pack::{
    bundled_bridge, pack_portable_zip, PackInputs, BRIDGE_CLI_PATH, BUNDLED_NODE_PATH,
    CONSOLE_EXE_NAME, ZIP_NAME,
};

fn fixture(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cursor2api-pack-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn inputs(root: &Path) -> PackInputs {
    let console = root.join("build").join("cursor2api.exe");
    let node = root.join("build").join("node.exe");
    let cli = root
        .join("build")
        .join("node_modules/cursor-api-proxy/dist/cli.js");
    let readme = root.join("README.md");
    write(&console, b"console-bytes");
    write(&node, b"node-bytes");
    write(&cli, b"bridge-cli");
    write(
        &readme,
        b"# cursor2api\n\n\xe4\xb8\x8d\xe6\x98\xaf\xe5\xae\x98\xe6\x96\xb9 Cursor\nCursor Account\n",
    );
    PackInputs {
        console_exe: console,
        node_exe: node,
        bridge_cli: cli,
        readme,
    }
}

fn zip_names(zip_path: &Path) -> Vec<String> {
    let file = fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut names = Vec::new();
    for i in 0..archive.len() {
        names.push(archive.by_index(i).unwrap().name().replace('\\', "/"));
    }
    names.sort();
    names
}

fn zip_bytes(zip_path: &Path, name: &str) -> Vec<u8> {
    let file = fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name(name).unwrap();
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap();
    buf
}

#[test]
fn release_artifact_is_a_zip_named_cursor2api_without_msi_or_nsis() {
    let root = fixture("zip-only");
    let dest = root.join("out");
    let zip_path = pack_portable_zip(&inputs(&root), &dest).expect("pack");
    assert_eq!(zip_path.file_name().unwrap(), ZIP_NAME);
    assert!(zip_path.is_file(), "Operator must get a zip they can unpack");
    let siblings: Vec<_> = fs::read_dir(&dest)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        siblings.iter().all(|name| {
            let lower = name.to_ascii_lowercase();
            !lower.ends_with(".msi") && !lower.contains("nsis") && !lower.ends_with(".exe")
        }),
        "Release dir must not grow an installer: {siblings:?}"
    );
}

#[test]
fn zip_contains_console_and_bundled_bridge_sidecar() {
    let root = fixture("sidecar");
    let zip_path = pack_portable_zip(&inputs(&root), &root.join("out")).unwrap();
    let names = zip_names(&zip_path);
    assert!(
        names.iter().any(|name| name == CONSOLE_EXE_NAME),
        "zip must contain the Console exe, got {names:?}"
    );
    assert!(
        names.iter().any(|name| name == BUNDLED_NODE_PATH),
        "zip must embed Node for the Bridge sidecar, got {names:?}"
    );
    assert!(
        names.iter().any(|name| name == BRIDGE_CLI_PATH),
        "zip must embed npm cursor-api-proxy, got {names:?}"
    );
    assert_eq!(zip_bytes(&zip_path, CONSOLE_EXE_NAME), b"console-bytes");
    assert_eq!(zip_bytes(&zip_path, BUNDLED_NODE_PATH), b"node-bytes");
    assert_eq!(zip_bytes(&zip_path, BRIDGE_CLI_PATH), b"bridge-cli");
}

#[test]
fn zip_does_not_ship_agent_cli_or_a_cursor_api_key() {
    let root = fixture("no-secrets");
    let pack = inputs(&root);
    write(&root.join("build/cursor-agent.exe"), b"agent-cli");
    write(&root.join("build/agent.exe"), b"agent-cli");
    write(
        &root.join("build/.env"),
        b"CURSOR_API_KEY=sk-should-never-ship\n",
    );
    write(
        &root.join("build/node_modules/cursor-api-proxy/.env"),
        b"CURSOR_API_KEY=sk-nested\n",
    );
    write(
        &root.join("build/node_modules/.bin/cursor-agent.exe"),
        b"agent-cli-in-modules",
    );
    write(
        &root.join("build/node_modules/.bin/agent.cmd"),
        b"agent-cli-cmd",
    );
    let zip_path = pack_portable_zip(&pack, &root.join("out")).unwrap();
    let names = zip_names(&zip_path);
    assert!(
        names.iter().all(|name| {
            let base = name.rsplit('/').next().unwrap_or(name);
            base != "cursor-agent.exe" && base != "agent.exe" && base != ".env"
        }),
        "zip must omit Agent CLI and env key files, got {names:?}"
    );
    for name in &names {
        let body = zip_bytes(&zip_path, name);
        let text = String::from_utf8_lossy(&body);
        assert!(
            !text.contains("CURSOR_API_KEY"),
            "{name} must not carry a Cursor API Key"
        );
    }
}

#[test]
fn zip_readme_says_unofficial_and_uses_the_operator_cursor_account() {
    let root = fixture("readme");
    let zip_path = pack_portable_zip(&inputs(&root), &root.join("out")).unwrap();
    let names = zip_names(&zip_path);
    assert!(
        names.iter().any(|name| name == "README.md"),
        "zip must include README.md, got {names:?}"
    );
    let text = String::from_utf8(zip_bytes(&zip_path, "README.md")).unwrap();
    assert!(
        text.contains("不是官方 Cursor") || text.contains("非 Cursor 官方"),
        "README must say this is not official Cursor, got: {text}"
    );
    assert!(
        text.contains("Cursor Account"),
        "README must say the Bridge spends the Operator Cursor Account, got: {text}"
    );
}

#[test]
fn unpacked_zip_layout_exposes_the_bundled_bridge() {
    let root = fixture("layout");
    let zip_path = pack_portable_zip(&inputs(&root), &root.join("out")).unwrap();
    let unpacked = root.join("unpacked");
    unzip_to(&zip_path, &unpacked);
    let found = bundled_bridge(&unpacked).expect("portable layout must be enough to start the Bridge");
    assert_eq!(found.node, unpacked.join("runtime/node.exe"));
    assert_eq!(found.cli, unpacked.join(BRIDGE_CLI_PATH));
    assert!(bundled_bridge(&root.join("empty")).is_none());
}

fn unzip_to(zip_path: &Path, dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    let file = fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().replace('\\', "/");
        let out = dest.join(name);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).unwrap();
        fs::write(out, buf).unwrap();
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn console_window_and_product_are_named_cursor2api() {
    let conf: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("src-tauri/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(conf["productName"], "cursor2api");
    assert_eq!(conf["app"]["windows"][0]["title"], "cursor2api");
}

#[test]
fn tauri_bundle_does_not_emit_msi_or_nsis() {
    let conf: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("src-tauri/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        conf["bundle"]["active"],
        false,
        "portable Release must not run the Tauri installer bundler"
    );
    let encoded = conf["bundle"]["targets"].to_string().to_ascii_lowercase();
    assert!(
        !encoded.contains("nsis") && !encoded.contains("msi"),
        "bundle.targets must not request MSI/NSIS, got {encoded}"
    );
}

#[test]
fn repo_readme_covers_unofficial_disclaimer_and_cursor_account() {
    let text = fs::read_to_string(repo_root().join("README.md")).unwrap();
    assert!(
        text.contains("不是官方 Cursor") || text.contains("非 Cursor 官方"),
        "README must disclaim official Cursor"
    );
    assert!(
        text.contains("Cursor Account"),
        "README must say the Operator Cursor Account is consumed"
    );
    assert!(
        text.contains("cursor2api-windows.zip") || text.contains("解压"),
        "README must tell an Operator they unpack a zip"
    );
    assert!(
        !text.contains("安装器"),
        "README must not call the Release an installer"
    );
    assert!(
        text.contains("两个") && text.contains("App Data"),
        "README must say two zip folders share App Data for one Windows login, got: {text}"
    );
}

#[test]
fn tray_icons_differ_when_the_bridge_is_running() {
    let stopped = fs::read(repo_root().join("src-tauri/icons/tray-stopped.png")).unwrap();
    let running = fs::read(repo_root().join("src-tauri/icons/tray-running.png")).unwrap();
    assert_ne!(
        stopped, running,
        "tray must use a different glyph while the Bridge is running"
    );
}
