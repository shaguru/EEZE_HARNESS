use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct PortablePaths {
    pub root: PathBuf,
    pub data: PathBuf,
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
            data.join("logs"),
            data.join("temp"),
            data.join("webview2"),
        ] {
            fs::create_dir_all(&dir).map_err(|e| format!("Cannot create writable portable folder {}: {e}. Move the app to a folder you own.", dir.display()))?;
        }
        let probe = data.join(".write-check");
        fs::write(&probe, b"ok").map_err(|e| {
            format!(
                "Portable data folder {} is not writable: {e}",
                data.display()
            )
        })?;
        fs::remove_file(probe).map_err(|e| e.to_string())?;
        // File-based auth is scoped to this portable CODEX_HOME, never the developer's keychain.
        let config = data.join("codex-home/config.toml");
        if !config.exists() {
            fs::write(&config, "cli_auth_credentials_store = \"file\"\napproval_policy = \"on-request\"\nsandbox_mode = \"read-only\"\n[analytics]\nenabled = false\n").map_err(|e| e.to_string())?;
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
        Ok(Self {
            home: data.join("codex-home"),
            data,
            root,
        })
    }
    pub fn load(&self) -> Result<Config, String> {
        let file = self.data.join("config.json");
        if !file.exists() {
            let c = Config::default();
            self.save(&c)?;
            return Ok(c);
        }
        let bytes = fs::read(&file).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Portable config is unexpectedly large".into());
        }
        let mut config:Config=serde_json::from_slice(&bytes).map_err(|e| format!("Invalid config.json: {e}. Restore config.json.bak or rename config.json to reset settings."))?;
        if config.approval_policy == "untrusted" {
            config.approval_policy = "on-request".into();
        }
        if config.schema_version != 1 {
            return Err("Unsupported config version; use the app version that created this portable folder.".into());
        }
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
        if file.exists() {
            fs::copy(&file, self.data.join("config.json.bak")).map_err(|e| e.to_string())?;
        }
        // ReplaceFileW is atomic on Windows, preserving the last valid file on failure.
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
            }
            let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        fs::rename(temp, file).map_err(|e| e.to_string())?;
        Ok(())
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
    pub labels: std::collections::BTreeMap<String, String>,
    pub debug_log: bool,
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
            labels: Default::default(),
            debug_log: false,
        }
    }
}

pub fn canonical_project(path: &str) -> Result<String, String> {
    let path = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("Cannot open project: {e}. Reconnect its folder if it moved."))?;
    if !path.is_dir() {
        return Err("Choose a folder, not a file".into());
    }
    fs::read_dir(&path).map_err(|e| format!("Project cannot be read: {e}"))?;
    let text = path.to_string_lossy();
    Ok(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
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
}
