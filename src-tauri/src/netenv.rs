//! Child-process environment and school-computer checks.
//!
//! Command-line agents do not read the Windows proxy settings that browsers use, so a school proxy
//! can let sign-in work in the browser while the agent itself cannot connect. This module copies the
//! user's Windows proxy into the child environment, adds an optional school CA certificate, and
//! strips variables that would silently change which account an agent uses.
use crate::portable_paths::{display_path, Config};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: String,
    /// "error", "warning" or "info".
    pub level: String,
    pub title: String,
    pub text: String,
}
impl Notice {
    fn new(id: &str, level: &str, title: &str, text: String) -> Self {
        Self {
            id: id.into(),
            level: level.into(),
            title: title.into(),
            text,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct ChildEnv {
    pub set: Vec<(String, String)>,
    pub remove: Vec<String>,
}
impl ChildEnv {
    pub fn apply(&self, cmd: &mut tokio::process::Command) {
        for k in &self.remove {
            cmd.env_remove(k);
        }
        for (k, v) in &self.set {
            cmd.env(k, v);
        }
    }
    fn set(&mut self, k: &str, v: impl Into<String>) {
        self.set.retain(|(key, _)| key != k);
        self.set.push((k.into(), v.into()));
    }
}

const PROXY_VARS: [&str; 6] = [
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "https_proxy",
    "http_proxy",
    "all_proxy",
];

fn env_proxy() -> Option<String> {
    PROXY_VARS
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

/// Proxy, certificate and temp-folder settings shared by both agents. Temp files go to the app
/// folder because on locked-down school accounts the Desktop may be the only writable place.
pub fn network_env(config: &Config, temp: Option<&Path>) -> ChildEnv {
    let mut env = ChildEnv::default();
    if let Some(temp) = temp.filter(|t| t.is_dir()) {
        let t = display_path(temp);
        env.set("TEMP", t.clone());
        env.set("TMP", t.clone());
        env.set("TMPDIR", t);
    }
    let mut bypass = vec!["localhost".to_string(), "127.0.0.1".into(), "::1".into()];
    match config.proxy.trim() {
        "" => {
            if env_proxy().is_none() {
                if let Some(p) = windows_proxy().filter(|p| !p.server.is_empty()) {
                    env.set("HTTPS_PROXY", p.server.clone());
                    env.set("HTTP_PROXY", p.server);
                    bypass.extend(p.bypass);
                }
            }
        }
        "none" => env.remove.extend(PROXY_VARS.iter().map(|s| s.to_string())),
        url => {
            env.set("HTTPS_PROXY", url);
            env.set("HTTP_PROXY", url);
        }
    }
    // Sign-in callbacks use localhost; they must never go through a proxy.
    if let Ok(existing) = std::env::var("NO_PROXY").or_else(|_| std::env::var("no_proxy")) {
        bypass.extend(existing.split(',').map(|s| s.trim().to_string()));
    }
    bypass.retain(|s| !s.is_empty());
    bypass.dedup();
    env.set("NO_PROXY", bypass.join(","));
    if let Some(ca) = config.ca_certificate.as_deref().filter(|c| Path::new(c).is_file()) {
        env.set("CODEX_CA_CERTIFICATE", ca);
        env.set("NODE_EXTRA_CA_CERTS", ca);
    }
    env
}

/// Environment for codex.exe: no API keys from the parent, so the portable sign-in is always used.
pub fn codex_env(config: &Config, temp: Option<&Path>) -> ChildEnv {
    let mut env = network_env(config, temp);
    env.remove
        .extend(["OPENAI_API_KEY", "CODEX_API_KEY"].map(String::from));
    env
}

/// Environment for claude.exe. Every CLAUDE*/ANTHROPIC* variable from the parent is removed: a teacher
/// launching the app from another Claude session, or a stray API key, would otherwise change the
/// account or refuse to start a nested session.
pub fn claude_env(
    config: &Config,
    home: &Path,
    git_bash: Option<&Path>,
    temp: Option<&Path>,
) -> ChildEnv {
    let mut env = network_env(config, temp);
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy().to_string();
        let upper = key.to_uppercase();
        if upper.starts_with("CLAUDE") || upper.starts_with("ANTHROPIC") {
            env.remove.push(key);
        }
    }
    env.remove
        .extend(["OPENAI_API_KEY", "CODEX_API_KEY"].map(String::from));
    env.set("CLAUDE_CONFIG_DIR", display_path(home));
    // A portable copy must not try to update itself into the user profile.
    env.set("DISABLE_AUTOUPDATER", "1");
    env.set("DISABLE_TELEMETRY", "1");
    env.set("DISABLE_ERROR_REPORTING", "1");
    env.set("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1");
    if let Some(bash) = git_bash {
        env.set("CLAUDE_CODE_GIT_BASH_PATH", display_path(bash));
    }
    env
}

pub struct WinProxy {
    pub server: String,
    pub bypass: Vec<String>,
    pub pac: Option<String>,
}

/// Reads the per-user Windows proxy (Settings → Network → Proxy). Read-only; nothing is changed.
#[cfg(windows)]
pub fn windows_proxy() -> Option<WinProxy> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
    let read_string = |name: &str| -> Option<String> {
        let name = wide(name);
        let mut buf = vec![0u16; 2048];
        let mut size = (buf.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as _,
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(0);
        Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.trim().is_empty())
    };
    let enabled = {
        let name = wide("ProxyEnable");
        let mut value = 0u32;
        let mut size = 4u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                &mut value as *mut u32 as _,
                &mut size,
            )
        };
        status == 0 && value == 1
    };
    let pac = read_string("AutoConfigURL");
    let server = if enabled {
        read_string("ProxyServer")
            .map(|s| parse_proxy_server(&s))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let bypass = read_string("ProxyOverride")
        .map(|s| {
            s.split(';')
                .map(str::trim)
                .filter(|x| !x.is_empty() && *x != "<local>")
                .map(|x| x.trim_start_matches("*.").to_string())
                .collect()
        })
        .unwrap_or_default();
    if server.is_empty() && pac.is_none() {
        return None;
    }
    Some(WinProxy {
        server,
        bypass,
        pac,
    })
}
#[cfg(not(windows))]
pub fn windows_proxy() -> Option<WinProxy> {
    None
}

/// "proxy:8080" or "http=a:80;https=b:443" → "http://b:443".
pub fn parse_proxy_server(raw: &str) -> String {
    let raw = raw.trim();
    let chosen = if raw.contains('=') {
        let parts: Vec<(&str, &str)> = raw
            .split(';')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect();
        parts
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("https"))
            .or_else(|| parts.iter().find(|(k, _)| k.eq_ignore_ascii_case("http")))
            .map(|(_, v)| *v)
            .unwrap_or("")
    } else {
        raw.split(';').next().unwrap_or("").trim()
    };
    if chosen.is_empty() {
        return String::new();
    }
    if chosen.contains("://") {
        chosen.to_string()
    } else {
        format!("http://{chosen}")
    }
}

pub fn proxy_summary(config: &Config) -> String {
    match config.proxy.trim() {
        "none" => "Direct connection (proxy turned off in Advanced settings)".into(),
        "" => {
            if let Some(p) = env_proxy() {
                format!("Automatic: environment proxy {p}")
            } else if let Some(p) = windows_proxy() {
                if !p.server.is_empty() {
                    format!("Automatic: Windows proxy {}", p.server)
                } else {
                    format!(
                        "Automatic: Windows uses a setup script ({}). Enter the proxy address in Advanced settings if the AI cannot connect.",
                        p.pac.unwrap_or_default()
                    )
                }
            } else {
                "Automatic: no proxy found (direct connection)".into()
            }
        }
        url => format!("Custom proxy {url}"),
    }
}

pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free = 0u64;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Some(free);
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// A Git for Windows copy shipped inside the app folder (optional). Claude uses it for shell commands.
pub fn bundled_git_bash(root: &Path) -> Option<PathBuf> {
    [
        "tools/PortableGit/bin/bash.exe",
        "PortableGit/bin/bash.exe",
        "tools/git/bin/bash.exe",
    ]
    .iter()
    .map(|p| root.join(p))
    .find(|p| p.is_file())
}

pub fn system_git_bash() -> Option<PathBuf> {
    let mut candidates = vec![];
    for var in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            let base = PathBuf::from(base);
            candidates.push(base.join("Git/bin/bash.exe"));
            candidates.push(base.join("Programs/Git/bin/bash.exe"));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("git.exe").is_file() {
                if let Some(root) = dir.parent() {
                    candidates.push(root.join("bin/bash.exe"));
                }
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn starts_with_ci(path: &str, prefix: &str) -> bool {
    let p = path.to_lowercase().replace('/', "\\");
    let q = prefix.to_lowercase().replace('/', "\\");
    !q.is_empty() && (p == q || p.starts_with(&(q.trim_end_matches('\\').to_string() + "\\")))
}

/// Problems a student is likely to hit on a managed school computer, found before they get confusing.
pub fn startup_notices(root: &Path, config: &Config) -> Vec<Notice> {
    let mut notices = vec![];
    let shown = display_path(root);
    let lower = shown.to_lowercase();
    let temp = std::env::var("TEMP").unwrap_or_default();
    let in_temp = starts_with_ci(&shown, &temp) || lower.contains("\\appdata\\local\\temp\\");
    if in_temp
        && (lower.contains(".zip")
            || lower.contains("\\temp1_")
            || lower.contains("\\rar$")
            || lower.contains("\\7z"))
    {
        notices.push(Notice::new("zip", "error", "Unzip the app first",
            "The app is running from inside the ZIP file, so it cannot keep your settings or find its helper programs. Close it, right-click the ZIP → Extract All…, choose your Desktop, then open RustPortableHarness.exe from the new folder.".into()));
    }
    let codex = root.join("codex.exe").is_file();
    let claude = root.join("claude.exe").is_file()
        || config
            .claude_binary_path
            .as_deref()
            .map(|p| Path::new(p).is_file())
            .unwrap_or(false)
        || user_claude_install().is_some();
    if !codex && !claude && config.binary_path.is_none() {
        notices.push(Notice::new("engines", "error", "AI programs are missing",
            "codex.exe and claude.exe were not found next to the app. Extract the complete ZIP again. If they disappear again, antivirus may be removing them — ask your teacher or IT to allow the folder.".into()));
    }
    for var in ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"] {
        if let Ok(dir) = std::env::var(var) {
            if starts_with_ci(&shown, &dir) {
                notices.push(Notice::new("onedrive", "warning", "This folder is synced by OneDrive",
                    "Your sign-in files and chats will be uploaded to OneDrive, and syncing can briefly lock files. If you can, keep the app in a folder that is not synced. Otherwise right-click the app folder and choose \"Always keep on this device\" so the programs are not removed to save space.".into()));
                break;
            }
        }
    }
    if let Some(free) = free_space(root) {
        if free < 500 * 1024 * 1024 {
            notices.push(Notice::new("disk", "warning", "Almost out of space",
                format!("Only {} MB is free where the app is stored. Saving chats or signing in may fail. Delete files you no longer need or ask for more storage.", free / (1024 * 1024))));
        }
    }
    if shown.starts_with(r"\\") {
        notices.push(Notice::new("network", "warning", "Running from a network folder",
            "Network folders can be slow or blocked for programs. If the AI fails to start, copy the app folder to your local Desktop.".into()));
    }
    if shown.chars().count() > 150 {
        notices.push(Notice::new("longpath", "info", "Folder path is very long",
            "Some tools fail with very long folder paths. If you see \"path not found\" errors, move the app folder closer to the top of the drive, for example Desktop\\Harness.".into()));
    }
    if config.proxy.is_empty() && env_proxy().is_none() {
        if let Some(p) = windows_proxy() {
            if p.server.is_empty() && p.pac.is_some() {
                notices.push(Notice::new("pac", "info", "School proxy uses a setup script",
                    "If sign-in works in the browser but the AI cannot connect, ask IT for the proxy address and enter it in Settings → Advanced → Network.".into()));
            }
        }
    }
    notices
}

/// Removes the app's own temp files older than a few days (best effort; failures are ignored).
pub fn clean_temp(dir: &Path, max_age_days: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let limit = std::time::Duration::from_secs(max_age_days * 86400);
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|age| age > limit)
            .unwrap_or(false);
        if !old {
            continue;
        }
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        } else {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The official native installer puts Claude here for the current user, without admin rights.
pub fn user_claude_install() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    let path = PathBuf::from(home).join(".local/bin/claude.exe");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_server_formats() {
        assert_eq!(parse_proxy_server("proxy.school:8080"), "http://proxy.school:8080");
        assert_eq!(
            parse_proxy_server("http=a:80;https=b:443;ftp=c:21"),
            "http://b:443"
        );
        assert_eq!(parse_proxy_server("http=a:80"), "http://a:80");
        assert_eq!(parse_proxy_server(""), "");
    }
    #[test]
    fn claude_env_strips_parent_claude_variables() {
        std::env::set_var("CLAUDECODE", "1");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test");
        let t = tempfile::tempdir().unwrap();
        let env = claude_env(&Config::default(), t.path(), None, Some(t.path()));
        assert!(env.remove.iter().any(|k| k == "CLAUDECODE"));
        assert!(env.remove.iter().any(|k| k == "ANTHROPIC_API_KEY"));
        assert!(env.set.iter().any(|(k, _)| k == "CLAUDE_CONFIG_DIR"));
        assert!(env.set.iter().any(|(k, v)| k == "NO_PROXY" && v.contains("localhost")));
        assert!(env.set.iter().any(|(k, _)| k == "TEMP"));
    }
    #[test]
    fn custom_and_disabled_proxy() {
        let mut c = Config::default();
        c.proxy = "http://proxy:3128".into();
        let env = network_env(&c, None);
        assert!(env.set.contains(&("HTTPS_PROXY".into(), "http://proxy:3128".into())));
        c.proxy = "none".into();
        let env = network_env(&c, None);
        assert!(env.remove.iter().any(|k| k == "HTTPS_PROXY"));
    }
    #[test]
    fn zip_temp_folder_is_detected() {
        let temp = std::env::var("TEMP").unwrap_or_else(|_| r"C:\Users\x\AppData\Local\Temp".into());
        let root = PathBuf::from(format!(r"{temp}\Temp1_RustPortableHarness.zip\RustPortableHarness"));
        let notices = startup_notices(&root, &Config::default());
        assert!(notices.iter().any(|n| n.id == "zip"));
    }
}
