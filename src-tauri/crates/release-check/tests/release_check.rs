use std::fs;
use std::path::PathBuf;

use cursor2api_release_check::{check_for_update, update_prompt, ReleaseSource};

fn newer_release_json() -> &'static str {
    r#"[{
        "tag_name": "v0.2.0",
        "draft": false,
        "prerelease": false,
        "html_url": "https://github.com/helloworldx55/cursor-api-proxy/releases/tag/v0.2.0",
        "assets": [{
            "name": "cursor2api-windows.zip",
            "browser_download_url": "https://github.com/helloworldx55/cursor-api-proxy/releases/download/v0.2.0/cursor2api-windows.zip"
        }]
    }]"#
}

#[test]
fn newer_release_yields_a_download_prompt() {
    let prompt = update_prompt("0.1.0", newer_release_json())
        .expect("valid Releases JSON")
        .expect("newer Release must prompt a download");
    assert_eq!(prompt.latest_version, "v0.2.0");
    assert_eq!(
        prompt.download_url,
        "https://github.com/helloworldx55/cursor-api-proxy/releases/download/v0.2.0/cursor2api-windows.zip"
    );
}

#[test]
fn matching_or_older_release_does_not_prompt() {
    assert_eq!(
        update_prompt("0.2.0", newer_release_json()).unwrap(),
        None,
        "same version must not nag"
    );
    assert_eq!(
        update_prompt("0.3.0", newer_release_json()).unwrap(),
        None,
        "running a newer Console must not prompt a download"
    );
}

#[test]
fn drafts_and_prereleases_are_ignored() {
    let json = r#"[
        {"tag_name":"v1.0.0","draft":true,"prerelease":false,"html_url":"https://example.invalid/draft","assets":[]},
        {"tag_name":"v0.9.0","draft":false,"prerelease":true,"html_url":"https://example.invalid/pre","assets":[]},
        {"tag_name":"v0.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/helloworldx55/cursor-api-proxy/releases/tag/v0.2.0","assets":[]}
    ]"#;
    let prompt = update_prompt("0.1.0", json)
        .unwrap()
        .expect("published Release after drafts");
    assert_eq!(prompt.latest_version, "v0.2.0");
    assert_eq!(
        prompt.download_url,
        "https://github.com/helloworldx55/cursor-api-proxy/releases/tag/v0.2.0"
    );
}

#[test]
fn prompt_uses_the_newest_published_release_not_the_first_row() {
    let json = r#"[
        {"tag_name":"v0.1.5","draft":false,"prerelease":false,"html_url":"https://github.com/helloworldx55/cursor-api-proxy/releases/tag/v0.1.5","assets":[]},
        {"tag_name":"v0.2.0","draft":false,"prerelease":false,"html_url":"https://github.com/helloworldx55/cursor-api-proxy/releases/tag/v0.2.0","assets":[]}
    ]"#;
    let prompt = update_prompt("0.1.0", json)
        .unwrap()
        .expect("newest published Release");
    assert_eq!(prompt.latest_version, "v0.2.0");
}

struct StaticReleases(&'static str);

impl ReleaseSource for StaticReleases {
    fn load_releases(&self) -> Result<String, String> {
        Ok(self.0.to_string())
    }
}

fn extract_tree(tag: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "cursor2api-release-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let exe = root.join("cursor2api.exe");
    fs::write(&exe, b"running-exe").unwrap();
    fs::write(root.join("payload.bin"), b"unzipped").unwrap();
    (root, exe)
}

#[test]
fn checking_releases_does_not_rewrite_the_running_exe_or_extract_dir() {
    let (root, exe) = extract_tree("no-overwrite");
    let prompt = check_for_update("0.1.0", &StaticReleases(newer_release_json()))
        .unwrap()
        .expect("prompt still appears");
    assert_eq!(prompt.latest_version, "v0.2.0");
    assert_eq!(fs::read(&exe).unwrap(), b"running-exe");
    assert_eq!(fs::read(root.join("payload.bin")).unwrap(), b"unzipped");
}
