//! Release 检查：只决定要不要提示下载，绝不改写 exe 或解压目录。

use serde::Deserialize;

pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/helloworldx55/cursor-api-proxy/releases";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpdatePrompt {
    pub latest_version: String,
    pub download_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn update_prompt(
    current_version: &str,
    releases_json: &str,
) -> Result<Option<UpdatePrompt>, String> {
    let releases: Vec<GithubRelease> =
        serde_json::from_str(releases_json).map_err(|err| err.to_string())?;
    let mut newest: Option<( (u64, u64, u64), GithubRelease )> = None;
    for release in releases {
        if release.draft || release.prerelease {
            continue;
        }
        let Some(version) = parse_version(&release.tag_name) else {
            continue;
        };
        match &newest {
            Some((best, _)) if version <= *best => {}
            _ => newest = Some((version, release)),
        }
    }
    let Some((latest, release)) = newest else {
        return Ok(None);
    };
    let current = parse_version(current_version)
        .ok_or_else(|| format!("无法解析当前版本 {current_version}"))?;
    if latest <= current {
        return Ok(None);
    }
    Ok(Some(UpdatePrompt {
        latest_version: release.tag_name.clone(),
        download_url: download_url(&release),
    }))
}

fn download_url(release: &GithubRelease) -> String {
    release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(".zip"))
        .map(|asset| asset.browser_download_url.clone())
        .unwrap_or_else(|| release.html_url.clone())
}

pub trait ReleaseSource {
    fn load_releases(&self) -> Result<String, String>;
}

pub struct GitHubReleaseSource {
    pub url: String,
}

impl Default for GitHubReleaseSource {
    fn default() -> Self {
        Self {
            url: RELEASES_API_URL.to_string(),
        }
    }
}

impl ReleaseSource for GitHubReleaseSource {
    fn load_releases(&self) -> Result<String, String> {
        http_agent()
            .get(&self.url)
            .set("User-Agent", "cursor2api")
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|err| err.to_string())?
            .into_string()
            .map_err(|err| err.to_string())
    }
}

fn http_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new();
    if let Ok(proxy) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("HTTP_PROXY")) {
        if let Ok(parsed) = ureq::Proxy::new(proxy.trim()) {
            builder = builder.proxy(parsed);
        }
    }
    builder.build()
}

pub fn check_for_update(
    current_version: &str,
    source: &impl ReleaseSource,
) -> Result<Option<UpdatePrompt>, String> {
    update_prompt(current_version, &source.load_releases()?)
}

fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    let mut parts = trimmed.split(['.', '-', '+']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}
