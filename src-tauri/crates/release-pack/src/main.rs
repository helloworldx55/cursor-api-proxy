use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use cursor2api_release_pack::{pack_portable_zip, PackInputs};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<std::path::PathBuf, String> {
    let mut console = None;
    let mut node = None;
    let mut bridge_cli = None;
    let mut readme = None;
    let mut out_dir = None;
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("缺少 {key} 的值"))?
            .clone();
        match key {
            "--console" => console = Some(PathBuf::from(value)),
            "--node" => node = Some(PathBuf::from(value)),
            "--bridge-cli" => bridge_cli = Some(PathBuf::from(value)),
            "--readme" => readme = Some(PathBuf::from(value)),
            "--out-dir" => out_dir = Some(PathBuf::from(value)),
            other => return Err(format!("未知参数 {other}")),
        }
        i += 2;
    }
    let inputs = PackInputs {
        console_exe: console.ok_or("--console 必填")?,
        node_exe: node.ok_or("--node 必填")?,
        bridge_cli: bridge_cli.ok_or("--bridge-cli 必填")?,
        readme: readme.ok_or("--readme 必填")?,
    };
    let dest = out_dir.ok_or("--out-dir 必填")?;
    pack_portable_zip(&inputs, &dest)
}
