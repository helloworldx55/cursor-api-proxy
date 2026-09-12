//! Portable zip：解压即可运行的未签名 Release，不含 Agent CLI 与 Cursor API Key。

use std::path::{Path, PathBuf};

#[cfg(feature = "archive")]
use std::fs::{self, File};
#[cfg(feature = "archive")]
use std::io::{Read, Write};
#[cfg(feature = "archive")]
use zip::write::SimpleFileOptions;
#[cfg(feature = "archive")]
use zip::CompressionMethod;
#[cfg(feature = "archive")]
use zip::ZipWriter;

pub const ZIP_NAME: &str = "cursor2api-windows.zip";
pub const CONSOLE_EXE_NAME: &str = "cursor2api.exe";
pub const BUNDLED_NODE_PATH: &str = "runtime/node.exe";
pub const BRIDGE_CLI_PATH: &str = "node_modules/cursor-api-proxy/dist/cli.js";

pub fn bundled_bridge(install_dir: &Path) -> Option<BundledBridge> {
    let node = install_dir.join("runtime").join("node.exe");
    let cli = install_dir.join(BRIDGE_CLI_PATH);
    if node.is_file() && cli.is_file() {
        Some(BundledBridge { node, cli })
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledBridge {
    pub node: PathBuf,
    pub cli: PathBuf,
}

pub struct PackInputs {
    pub console_exe: PathBuf,
    pub node_exe: PathBuf,
    pub bridge_cli: PathBuf,
    pub readme: PathBuf,
}

#[cfg(feature = "archive")]
pub fn pack_portable_zip(inputs: &PackInputs, dest_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dest_dir).map_err(|err| err.to_string())?;
    let zip_path = dest_dir.join(ZIP_NAME);
    let file = File::create(&zip_path).map_err(|err| err.to_string())?;
    let mut zip = ZipWriter::new(file);
    add_file(&mut zip, CONSOLE_EXE_NAME, &inputs.console_exe)?;
    add_file(&mut zip, BUNDLED_NODE_PATH, &inputs.node_exe)?;
    add_tree(
        &mut zip,
        &node_modules_dir(&inputs.bridge_cli)?,
        "node_modules",
    )?;
    add_file(&mut zip, "README.md", &inputs.readme)?;
    zip.finish().map_err(|err| err.to_string())?;
    Ok(zip_path)
}

#[cfg(feature = "archive")]
fn node_modules_dir(bridge_cli: &Path) -> Result<PathBuf, String> {
    let mut current = bridge_cli.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|name| name.to_str()) == Some("node_modules") {
            return Ok(dir.to_path_buf());
        }
        current = dir.parent();
    }
    Err(format!(
        "Bridge sidecar 不在 node_modules 下：{}",
        bridge_cli.display()
    ))
}

#[cfg(feature = "archive")]
fn add_tree(zip: &mut ZipWriter<File>, src: &Path, zip_prefix: &str) -> Result<(), String> {
    if src.is_file() {
        return add_file(zip, zip_prefix, src);
    }
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if omit_entry(&name, entry.path().is_file()) {
            continue;
        }
        let child_prefix = format!("{zip_prefix}/{name}");
        add_tree(zip, &entry.path(), &child_prefix)?;
    }
    Ok(())
}

#[cfg(feature = "archive")]
fn omit_entry(name: &str, is_file: bool) -> bool {
    if matches!(name, ".env" | ".env.local" | ".env.production")
        || name.ends_with(".msi")
        || name.ends_with(".nsi")
        || name.eq_ignore_ascii_case("nsis")
    {
        return true;
    }
    matches!(
        name,
        "cursor-agent"
            | "cursor-agent.exe"
            | "cursor-agent.cmd"
            | "cursor-agent.ps1"
            | "agent.exe"
            | "agent.cmd"
            | "agent.ps1"
    ) || (is_file && name == "agent")
}

#[cfg(feature = "archive")]
fn add_file(zip: &mut ZipWriter<File>, name: &str, src: &Path) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(name, options).map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    File::open(src)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|err| err.to_string())?;
    zip.write_all(&bytes).map_err(|err| err.to_string())
}
