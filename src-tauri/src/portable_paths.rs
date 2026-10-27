use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const CODEX_DEFAULT: &str = "codex-default";
pub const CLAUDE_DEFAULT: &str = "claude-default";
const CODEX_HOME_CONFIG: &str = "cli_auth_credentials_store = \"file\"\napproval_policy = \"on-request\"\nsandbox_mode = \"read-only\"\n[analytics]\nenabled = false\n";

#[derive(Clone, Debug)]
pub struct PortablePaths {
    pub root: PathBuf,
    pub data: PathBuf,
    /// Home of the original single Codex account. Kept at its old location so upgrades keep the sign-in.
    pub home: PathBuf,
}
impl PortablePaths {
    pub fn discover() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Self::at(
            exe.parent()
                .ok_or("Executable has no parent folder")?
                .to_path_buf(),
        )
    }
    pub fn at(root: PathBuf) -> Result<Self, String> {
        let data = root.join("data");
        for dir in [
            data.clone(),
            data.join("codex-home"),
            data.join("claude-home"),
            data.join("accounts"),
            data.join("logs"),
            data.join("temp"),
            data.join("webview2"),
        ] {
            fs::create_dir_all(&dir).map_err(|e| format!("Cannot create the app's data folder {}: {e}.\n\nMove the whole app folder to a place you can write to, such as your Desktop. If Windows Security showed \"Unauthorized changes blocked\", Controlled folder access is protecting this folder: ask IT to allow RustPortableHarness.exe, or use a USB drive.", dir.display()))?;
        }
        let probe = data.join(".write-check");
        fs::write(&probe, b"ok").map_err(|e| {
            format!(
                "The app's data folder {} is read-only: {e}.\n\nMove the whole app folder to a place you can write to, such as your Desktop. If Windows Security showed \"Unauthorized changes blocked\", Controlled folder access is protecting this folder: ask IT to allow RustPortableHarness.exe, or use a USB drive.",
                data.display()
            )
        })?;
        fs::remove_file(probe).map_err(|e| e.to_string())?;
        let paths = Self {
            home: data.join("codex-home"),
            data,
            root,
        };
        paths.ensure_codex_home(&paths.home)?;
        Ok(paths)
    }
    /// File-based auth keeps Codex sign-in inside this portable folder, never the developer's keychain.
    fn ensure_codex_home(&self, home: &Path) -> Result<(), String> {
        fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let config = home.join("config.toml");
        if !config.exists() {
            fs::write(&config, CODEX_HOME_CONFIG).map_err(|e| e.to_string())?;
        } else {
            let text = fs::read_to_string(&config).map_err(|e| e.to_string())?;
            if text.contains("approval_policy = \"untrusted\"") {
                fs::write(
                    &config,
                    text.replace(
                        "approval_policy = \"untrusted\"",
                        "approval_policy = \"on-request\"",
                    ),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    /// Each account gets its own home folder so sign-ins and conversations never mix.
    pub fn account_home(&self, account: &Account) -> Result<PathBuf, String> {
        if !valid_account_id(&account.id) {
            return Err("Invalid account id".into());
        }
        let home = match account.id.as_str() {
            CODEX_DEFAULT => self.home.clone(),
            CLAUDE_DEFAULT => self.data.join("claude-home"),
            id => self.data.join("accounts").join(id),
        };
        if account.provider == "codex" {
            self.ensure_codex_home(&home)?;
        } else {
            fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        }
        Ok(home)
    }
    /// Holding this file open (no sharing) stops two copies of the app from using the same data folder.
    pub fn lock_instance(&self) -> Result<fs::File, String> {
        let path = self.data.join(".instance.lock");
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        options.open(&path).map_err(|e| {
            if e.raw_os_error() == Some(32) {
                "RustPortableHarness is already open from this folder. Switch to the open window (check the taskbar).".to_string()
            } else {
                format!("Cannot lock the data folder: {e}")
            }
        })
    }
    pub fn load(&self) -> Result<Config, String> {
        let file = self.data.join("config.json");
        if !file.exists() {
            let mut c = Config::default();
            c.normalize();
            self.save(&c)?;
            return Ok(c);
        }
        let bytes = fs::read(&file).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Portable config is unexpectedly large".into());
        }
        // Notepad may save a byte-order mark when someone edits the file by hand.
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        let mut config:Config=serde_json::from_slice(bytes).map_err(|e| format!("Invalid config.json: {e}. Restore config.json.bak or rename config.json to reset settings."))?;
        if config.schema_version != 1 {
            return Err("Unsupported config version; use the app version that created this portable folder.".into());
        }
        config.normalize();
        Ok(config)
    }
    pub fn save(&self, config: &Config) -> Result<(), String> {
        let file = self.data.join("config.json");
        let temp = self.data.join("config.json.tmp");
        let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut f = fs::File::create(&temp).map_err(|e| e.to_string())?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        if file.exists() {
            let _ = fs::copy(&file, self.data.join("config.json.bak"));
        }
        // Sync tools (OneDrive) and antivirus can hold the file briefly; retry before giving up.
        let mut last = String::new();
        for attempt in 0..5 {
            match replace_file(&temp, &file) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
            std::thread::sleep(std::time::Duration::from_millis(60 * (attempt + 1)));
        }
        Err(format!("Could not save settings ({last}). If the folder is synced by OneDrive or scanned by antivirus, try again in a moment."))
    }
}

fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    // MoveFileExW with REPLACE_EXISTING | WRITE_THROUGH keeps the last valid file on failure.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0x1 | 0x8) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(from, to).map_err(|e| e.to_string())
}

pub fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// "codex" (ChatGPT sign-in) or "claude" (Claude sign-in).
    pub provider: String,
    pub label: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    /// Last seen usage: {primary:{usedPercent,windowMins,resetsAt}, secondary:{...}, limitReached}.
    pub usage: Option<Value>,
    pub usage_at: Option<u64>,
}
impl Account {
    pub fn new(id: &str, provider: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            provider: provider.into(),
            label: label.into(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub schema_version: u32,
    pub projects: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_thread: Option<String>,
    pub binary_path: Option<String>,
    pub model: Option<String>,
    pub theme: String,
    pub font_size: u8,
    pub sandbox: String,
    pub approval_policy: String,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: String,
    pub service_tier: Option<String>,
    /// Composer dock: "bottom", "left" or "right".
    pub layout: String,
    pub sidebar_width: u16,
    pub inspector_width: u16,
    pub labels: std::collections::BTreeMap<String, String>,
    pub debug_log: bool,
    /// Active engine: "codex" or "claude".
    pub provider: String,
    pub accounts: Vec<Account>,
    pub active_codex_account: String,
    pub active_claude_account: String,
    pub claude_binary_path: Option<String>,
    pub claude_model: Option<String>,
    pub claude_effort: Option<String>,
    /// Project `.claude/settings*.json` can define hooks that run commands; off unless a teacher enables it.
    pub claude_project_settings: bool,
    pub composer_width: u16,
    pub composer_height: u16,
    pub show_inspector: bool,
    pub auto_connect: bool,
    /// "" = automatic (environment, then Windows proxy settings), "none" = direct, otherwise a proxy URL.
    pub proxy: String,
    pub ca_certificate: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            projects: vec![],
            selected_project: None,
            selected_thread: None,
            binary_path: None,
            model: None,
            theme: "dark".into(),
            font_size: 13,
            sandbox: "read-only".into(),
            approval_policy: "on-request".into(),
            reasoning_effort: None,
            reasoning_summary: "auto".into(),
            service_tier: None,
            layout: "bottom".into(),
            sidebar_width: 235,
            inspector_width: 295,
            labels: Default::default(),
            debug_log: false,
            provider: "codex".into(),
            accounts: vec![],
            active_codex_account: CODEX_DEFAULT.into(),
            active_claude_account: CLAUDE_DEFAULT.into(),
            claude_binary_path: None,
            claude_model: None,
            claude_effort: None,
            claude_project_settings: false,
            composer_width: 360,
            composer_height: 170,
            show_inspector: true,
            auto_connect: true,
            proxy: String::new(),
            ca_certificate: None,
        }
    }
}
impl Config {
    /// Repairs values from older versions or hand edits so the app always starts in a safe, valid state.
    pub fn normalize(&mut self) {
        if self.approval_policy == "untrusted" || !["on-request", "never"].contains(&self.approval_policy.as_str()) {
            self.approval_policy = "on-request".into();
        }
        if !["read-only", "workspace-write", "danger-full-access"].contains(&self.sandbox.as_str()) {
            self.sandbox = "read-only".into();
        }
        if !["auto", "concise", "detailed", "none"].contains(&self.reasoning_summary.as_str()) {
            self.reasoning_summary = "auto".into();
        }
        if !["bottom", "left", "right"].contains(&self.layout.as_str()) {
            self.layout = "bottom".into();
        }
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            self.theme = "dark".into();
        }
        self.font_size = self.font_size.clamp(11, 20);
        if !["codex", "claude"].contains(&self.provider.as_str()) {
            self.provider = "codex".into();
        }
        self.sidebar_width = self.sidebar_width.clamp(180, 420);
        self.inspector_width = self.inspector_width.clamp(220, 460);
        self.composer_width = self.composer_width.clamp(260, 720);
        self.composer_height = self.composer_height.clamp(110, 520);
        self.accounts.retain(|a| {
            valid_account_id(&a.id) && ["codex", "claude"].contains(&a.provider.as_str())
        });
        let mut seen = std::collections::BTreeSet::new();
        self.accounts.retain(|a| seen.insert(a.id.clone()));
        if !self.accounts.iter().any(|a| a.id == CODEX_DEFAULT) {
            self.accounts
                .insert(0, Account::new(CODEX_DEFAULT, "codex", "ChatGPT account"));
        }
        if !self.accounts.iter().any(|a| a.id == CLAUDE_DEFAULT) {
            self.accounts
                .push(Account::new(CLAUDE_DEFAULT, "claude", "Claude account"));
        }
        for a in &mut self.accounts {
            if a.id == CODEX_DEFAULT {
                a.provider = "codex".into();
            }
            if a.id == CLAUDE_DEFAULT {
                a.provider = "claude".into();
            }
            if a.label.trim().is_empty() {
                a.label = if a.provider == "codex" { "ChatGPT account" } else { "Claude account" }.into();
            }
        }
        if !self.has_account(&self.active_codex_account.clone(), "codex") {
            self.active_codex_account = CODEX_DEFAULT.into();
        }
        if !self.has_account(&self.active_claude_account.clone(), "claude") {
            self.active_claude_account = CLAUDE_DEFAULT.into();
        }
        if let Some(effort) = &self.claude_effort {
            if !CLAUDE_EFFORTS.contains(&effort.as_str()) {
                self.claude_effort = None;
            }
        }
    }
    fn has_account(&self, id: &str, provider: &str) -> bool {
        self.accounts
            .iter()
            .any(|a| a.id == id && a.provider == provider)
    }
    pub fn active_account(&self, provider: &str) -> Account {
        let id = if provider == "claude" {
            &self.active_claude_account
        } else {
            &self.active_codex_account
        };
        self.accounts
            .iter()
            .find(|a| &a.id == id)
            .cloned()
            .unwrap_or_else(|| {
                if provider == "claude" {
                    Account::new(CLAUDE_DEFAULT, "claude", "Claude account")
                } else {
                    Account::new(CODEX_DEFAULT, "codex", "ChatGPT account")
                }
            })
    }
    pub fn account_mut(&mut self, id: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.id == id)
    }
}

pub const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn canonical_project(path: &str) -> Result<String, String> {
    let path = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("Cannot open project: {e}. Reconnect its folder if it moved."))?;
    if !path.is_dir() {
        return Err("Choose a folder, not a file".into());
    }
    fs::read_dir(&path).map_err(|e| format!("Project cannot be read: {e}"))?;
    Ok(display_path(&path))
}

/// Removes the `\\?\` verbatim prefix from canonical Windows paths, keeping network shares valid.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

/// Folders that are too broad to hand to an AI agent: drive roots, the user profile, Desktop, Documents.
pub fn broad_folder_warning(path: &str) -> Option<String> {
    if Path::new(path).parent().is_none() {
        return Some("This is a whole drive. Choose a single project folder instead so the AI only sees that project.".into());
    }
    let norm = |s: &str| s.trim_end_matches(['\\', '/']).to_lowercase();
    let target = norm(path);
    let mut broad = vec![];
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = home.to_string_lossy().to_string();
        broad.push(norm(&home));
        for sub in ["Desktop", "Documents", "Downloads", "OneDrive"] {
            broad.push(norm(&format!("{home}\\{sub}")));
        }
    }
    for var in ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"] {
        if let Some(v) = std::env::var_os(var) {
            let v = v.to_string_lossy().to_string();
            broad.push(norm(&v));
            broad.push(norm(&format!("{v}\\Desktop")));
            broad.push(norm(&format!("{v}\\Documents")));
        }
    }
    if broad.iter().any(|b| !b.is_empty() && *b == target) {
        return Some("This folder holds many of your files. Make a new folder for each project (for example Desktop\\my-game) so the AI only sees that project.".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_roundtrip_and_backup() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        let mut c = p.load().unwrap();
        c.model = Some("from-model-list".into());
        p.save(&c).unwrap();
        assert_eq!(p.load().unwrap().model, c.model);
        assert!(p.data.join("config.json.bak").exists());
    }
    #[test]
    fn invalid_project() {
        assert!(canonical_project("Z:\\a-folder-that-does-not-exist-7462").is_err());
    }
    #[test]
    fn project_must_be_directory() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("file");
        fs::write(&f, b"a").unwrap();
        assert!(canonical_project(f.to_str().unwrap()).is_err());
    }
    #[test]
    fn corrupt_config_is_not_overwritten() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        fs::write(p.data.join("config.json"), b"bad").unwrap();
        assert!(p.load().is_err());
        assert_eq!(fs::read(p.data.join("config.json")).unwrap(), b"bad");
    }
    #[test]
    fn old_config_gains_default_accounts_and_safe_values() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        fs::write(
            p.data.join("config.json"),
            br#"{"schemaVersion":1,"approvalPolicy":"untrusted","layout":"diagonal","composerWidth":5,"provider":"other"}"#,
        )
        .unwrap();
        let c = p.load().unwrap();
        assert_eq!(c.approval_policy, "on-request");
        assert_eq!(c.layout, "bottom");
        assert_eq!(c.provider, "codex");
        assert_eq!(c.composer_width, 260);
        assert_eq!(c.active_account("codex").id, CODEX_DEFAULT);
        assert_eq!(c.active_account("claude").id, CLAUDE_DEFAULT);
    }
    #[test]
    fn notepad_byte_order_mark_is_accepted() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        fs::write(p.data.join("config.json"), b"\xEF\xBB\xBF{\"schemaVersion\":1,\"theme\":\"light\"}").unwrap();
        assert_eq!(p.load().unwrap().theme, "light");
    }
    #[test]
    fn account_homes_are_separate_and_inside_data() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        let codex = p.account_home(&Account::new(CODEX_DEFAULT, "codex", "a")).unwrap();
        let claude = p.account_home(&Account::new(CLAUDE_DEFAULT, "claude", "b")).unwrap();
        let extra = p.account_home(&Account::new("codex-2", "codex", "c")).unwrap();
        assert_eq!(codex, p.home);
        assert_ne!(claude, codex);
        assert!(extra.starts_with(p.data.join("accounts")));
        assert!(extra.join("config.toml").exists());
        assert!(p.account_home(&Account::new("../evil", "codex", "x")).is_err());
    }
    #[test]
    fn second_instance_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let p = PortablePaths::at(t.path().to_path_buf()).unwrap();
        let _first = p.lock_instance().unwrap();
        #[cfg(windows)]
        assert!(p.lock_instance().is_err());
    }
    #[test]
    fn unc_paths_keep_their_prefix() {
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\proj")),
            r"\\server\share\proj"
        );
        assert_eq!(display_path(Path::new(r"\\?\C:\proj")), r"C:\proj");
    }
    #[test]
    fn whole_drive_is_flagged() {
        assert!(broad_folder_warning(r"C:\").is_some());
        assert!(broad_folder_warning(r"C:\Users\someone\Desktop\my-game-7731").is_none());
    }
}
