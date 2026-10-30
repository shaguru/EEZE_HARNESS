//! Portable programming tools for classrooms where nothing can be installed.
//!
//! Python (via uv), Rust (rustup's windows-gnu toolchain plus a few MinGW binutils), Git
//! (PortableGit), PowerShell 7 and Node.js are downloaded from their official release pages,
//! checked against pinned SHA-256 checksums and unpacked into `<app>/tools`. Settings and caches go
//! to `<app>/data`. Nothing is written to Windows, the registry or the user profile. The AI agents,
//! project setup, Run and Terminal all use the same environment from [`env`].
use crate::{
    netenv::ChildEnv,
    portable_paths::display_path,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncBufReadExt;

pub struct Download {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub file: &'static str,
}

const UV: Download = Download {
    url: "https://github.com/astral-sh/uv/releases/download/0.13.6/uv-x86_64-pc-windows-msvc.zip",
    sha256: "a2a8820d5eea32f08ef467ef3b7f66883a9b076d5737a2fc2fbb5f458d1bb50f",
    size: 15_239_500,
    file: "uv-0.13.6.zip",
};
/// The python-build-standalone build that uv itself would install (includes tkinter). The app
/// unpacks it straight into its final folder: uv's own installer renames a freshly unpacked folder,
/// which antivirus on school PCs often blocks ("Access is denied"), and it also writes to the
/// registry and the user profile.
const PYTHON: Download = Download {
    url: "https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.14.10%2B20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
    sha256: "c1876a9795d3a94ee49a1499a69cd91e26ca3e2829593f93fa6e4db90d05402b",
    size: 23_294_761,
    file: "cpython-3.14.10-20261026-windows-x86_64.tar.gz",
};
/// The folder name uv uses for this build, so `uv` finds it as a managed Python.
const PYTHON_DIR: &str = "cpython-3.14.10-windows-x86_64-none";
const RUSTUP: Download = Download {
    url: "https://static.rust-lang.org/rustup/archive/1.29.2/x86_64-pc-windows-msvc/rustup-init.exe",
    sha256: "00f139da614dced87d3c3d4121c43eebdac3c1bf8a5ee86b9d80dfd71c8d70fd",
    size: 12_724_224,
    file: "rustup-init-1.29.2.exe",
};
/// Only the assembler, dlltool, ld and their DLLs are kept from this archive. Rust needs them to
/// build programs that call Windows APIs (all window apps).
const WINLIBS: Download = Download {
    url: "https://github.com/brechtsanders/winlibs_mingw/releases/download/16.2.0posix-14.0.0-msvcrt-r2/winlibs-x86_64-posix-seh-gcc-16.2.0-mingw-w64msvcrt-14.0.0-r2.zip",
    sha256: "1b90ec73c96e6905a913746b56fb6b20122422787029810d3afc6a3b440f2322",
    size: 274_281_827,
    file: "winlibs-16.2.0-msvcrt-r2.zip",
};
const BINUTILS: [&str; 8] = [
    "as.exe",
    "dlltool.exe",
    "ld.exe",
    "libintl-8.dll",
    "libiconv-2.dll",
    "zlib1.dll",
    "libzstd.dll",
    "libwinpthread-1.dll",
];
const GIT: Download = Download {
    url: "https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.4/PortableGit-2.56.0.4-64-bit.7z.exe",
    sha256: "b1f3e52216e2ed6882d1bebfd4d3fdf319bec40d2edbbf9fd26ec4fd3fce7ed1",
    size: 60_087_472,
    file: "PortableGit-2.56.0.4-64-bit.7z.exe",
};
const PWSH: Download = Download {
    url: "https://github.com/PowerShell/PowerShell/releases/download/v7.6.9/PowerShell-7.6.9-win-x64.zip",
    sha256: "ce7484804c0f0ad4dda10c958e71acdaf7eebfb22505c1b480f89e0e9f1da94d",
    size: 109_620_855,
    file: "PowerShell-7.6.9-win-x64.zip",
};
const NODE: Download = Download {
    url: "https://nodejs.org/dist/v26.13.0/node-v26.13.0-win-x64.zip",
    sha256: "dcc60d59a0746999f99064a01c020445d54d84228c691cdbf25b49ac3b59150a",
    size: 41_157_375,
    file: "node-v26.13.0-win-x64.zip",
};
pub const PYTHON_VERSION: &str = "3.14";
const RUST_TOOLCHAIN: &str = "stable-x86_64-pc-windows-gnu";
/// Paths near Windows' classic 260-character limit break the Rust linker and build scripts.
const RUST_PATH_LIMIT: usize = 250;

pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub purpose: &'static str,
    pub download_mb: u32,
    pub disk_mb: u32,
}
pub const TOOLS: [Tool; 5] = [
    Tool { id: "python", name: "Python 3.14 (with tkinter)", purpose: "Python programs and tkinter window apps. Every project gets its own environment.", download_mb: 45, disk_mb: 110 },
    Tool { id: "rust", name: "Rust", purpose: "Rust console and window apps with cargo. No Visual Studio needed.", download_mb: 380, disk_mb: 850 },
    Tool { id: "git", name: "Git", purpose: "Version control, plus the Bash shell Claude uses to run commands.", download_mb: 58, disk_mb: 360 },
    Tool { id: "pwsh", name: "PowerShell 7", purpose: "A modern command shell for the AI and the Terminal button.", download_mb: 105, disk_mb: 250 },
    Tool { id: "node", name: "Node.js (npm, npx, Convex CLI)", purpose: "JavaScript projects and deploying to Convex with npx convex.", download_mb: 40, disk_mb: 120 },
];

/// Where every tool, cache and setting lives inside the portable folder.
#[derive(Clone, Debug)]
pub struct Layout {
    pub root: PathBuf,
    pub data: PathBuf,
}
impl Layout {
    pub fn new(root: &Path, data: &Path) -> Self {
        Self { root: root.to_path_buf(), data: data.to_path_buf() }
    }
    pub fn tools(&self) -> PathBuf {
        self.root.join("tools")
    }
    fn uv_dir(&self) -> PathBuf {
        self.tools().join("uv")
    }
    fn uv(&self) -> PathBuf {
        self.uv_dir().join("uv.exe")
    }
    fn python_home(&self) -> PathBuf {
        self.tools().join("python")
    }
    /// The uv-managed CPython folder (contains python.exe), newest first.
    pub fn python_dir(&self) -> Option<PathBuf> {
        let mut dirs: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(self.python_home())
            .ok()?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let version = name.strip_prefix("cpython-")?.split('-').next()?.to_string();
                let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
                (parts.len() == 3 && e.path().join("python.exe").is_file()).then(|| (parts, e.path()))
            })
            .collect();
        dirs.sort();
        dirs.pop().map(|(_, p)| p)
    }
    fn rust_dir(&self) -> PathBuf {
        self.tools().join("rust")
    }
    pub fn cargo_home(&self) -> PathBuf {
        self.rust_dir().join("cargo")
    }
    pub fn rustup_home(&self) -> PathBuf {
        self.rust_dir().join("rustup")
    }
    fn cargo_bin(&self) -> PathBuf {
        self.cargo_home().join("bin")
    }
    fn binutils(&self) -> PathBuf {
        self.rust_dir().join("binutils")
    }
    fn rust_linker(&self) -> PathBuf {
        self.rustup_home()
            .join("toolchains")
            .join(RUST_TOOLCHAIN)
            .join("lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained/x86_64-w64-mingw32-gcc.exe")
    }
    fn git_dir(&self) -> PathBuf {
        self.tools().join("git")
    }
    pub fn git_exe(&self) -> PathBuf {
        self.git_dir().join("cmd").join("git.exe")
    }
    fn pwsh_dir(&self) -> PathBuf {
        self.tools().join("pwsh")
    }
    fn pwsh(&self) -> PathBuf {
        self.pwsh_dir().join("pwsh.exe")
    }
    fn node_dir(&self) -> PathBuf {
        self.tools().join("node")
    }
    fn node(&self) -> PathBuf {
        self.node_dir().join("node.exe")
    }
    fn npm_global(&self) -> PathBuf {
        self.data.join("npm-global")
    }
    fn downloads(&self) -> PathBuf {
        self.data.join("downloads")
    }
    fn cache(&self) -> PathBuf {
        self.data.join("cache")
    }
    fn gitconfig(&self) -> PathBuf {
        self.data.join("gitconfig")
    }
    pub fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }
    fn rust_path_too_long(&self) -> bool {
        display_path(&self.rust_linker()).chars().count() > RUST_PATH_LIMIT
    }
}

fn on_path(exe: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

/// "ready" (in the app folder), "system" (installed on this PC), "partial" or "missing".
pub fn status(l: &Layout, id: &str) -> (&'static str, String) {
    match id {
        "python" => match (l.uv().is_file(), l.python_dir()) {
            (true, Some(dir)) => ("ready", format!("uv + {}", dir.file_name().unwrap_or_default().to_string_lossy())),
            (true, None) => ("partial", "uv is here but Python is not; get it again".into()),
            _ => ("missing", String::new()),
        },
        "rust" => {
            let cargo = l.cargo_bin().join("cargo.exe").is_file();
            let rustc = l.rustup_home().join("toolchains").join(RUST_TOOLCHAIN).join("bin/rustc.exe").is_file();
            if cargo && rustc {
                if l.rust_path_too_long() {
                    ("partial", "the app folder path is too long for Rust; move it closer to the drive, e.g. Desktop\\Harness".into())
                } else if l.binutils().join("as.exe").is_file() {
                    ("ready", RUST_TOOLCHAIN.into())
                } else {
                    ("partial", "console apps only; get it again for window apps".into())
                }
            } else {
                ("missing", String::new())
            }
        }
        "git" => {
            if l.git_exe().is_file() {
                ("ready", "PortableGit 2.56".into())
            } else if let Some(p) = on_path("git.exe") {
                ("system", display_path(&p))
            } else {
                ("missing", String::new())
            }
        }
        "pwsh" => {
            if l.pwsh().is_file() {
                ("ready", "PowerShell 7.6".into())
            } else if let Some(p) = on_path("pwsh.exe") {
                ("system", display_path(&p))
            } else {
                ("missing", String::new())
            }
        }
        "node" => {
            if l.node().is_file() {
                ("ready", "Node.js 26 LTS".into())
            } else if let Some(p) = on_path("node.exe") {
                ("system", display_path(&p))
            } else {
                ("missing", String::new())
            }
        }
        _ => ("missing", String::new()),
    }
}
pub fn usable(l: &Layout, id: &str) -> bool {
    matches!(status(l, id).0, "ready" | "system") || (id == "rust" && status(l, id).0 == "partial" && !l.rust_path_too_long())
}
pub fn statuses(l: &Layout) -> Value {
    json!(TOOLS
        .iter()
        .map(|t| {
            let (state, detail) = status(l, t.id);
            json!({"id": t.id, "name": t.name, "purpose": t.purpose, "downloadMb": t.download_mb, "diskMb": t.disk_mb, "state": state, "detail": detail})
        })
        .collect::<Vec<_>>())
}

/// A small stamp of what is installed, so running agents can tell when to restart.
pub fn fingerprint(l: &Layout) -> String {
    TOOLS.iter().map(|t| status(l, t.id).0).collect::<Vec<_>>().join(",")
}

/// PATH entries and settings that make every installed tool work from the app folder only.
pub fn env(l: &Layout) -> ChildEnv {
    let mut env = ChildEnv::default();
    let mut dirs = vec![];
    if l.uv().is_file() {
        dirs.push(l.uv_dir());
    }
    if let Some(py) = l.python_dir() {
        dirs.push(py.join("Scripts"));
        dirs.push(py);
    }
    if l.cargo_bin().is_dir() {
        dirs.push(l.cargo_bin());
    }
    if l.binutils().is_dir() {
        dirs.push(l.binutils());
    }
    if l.git_exe().is_file() {
        dirs.push(l.git_dir().join("cmd"));
    }
    if l.node().is_file() {
        dirs.push(l.node_dir());
        dirs.push(l.npm_global());
    }
    if l.pwsh().is_file() {
        dirs.push(l.pwsh_dir());
    }
    env.prepend_path(&dirs);
    let p = |p: PathBuf| display_path(&p);
    let cache = l.cache();
    for (k, v) in [
        ("CARGO_HOME", p(l.cargo_home())),
        ("RUSTUP_HOME", p(l.rustup_home())),
        ("UV_PYTHON_INSTALL_DIR", p(l.python_home())),
        ("UV_PYTHON_BIN_DIR", p(l.python_home().join("bin"))),
        ("UV_CACHE_DIR", p(cache.join("uv"))),
        ("UV_TOOL_DIR", p(l.data.join("uv-tools"))),
        ("UV_TOOL_BIN_DIR", p(l.data.join("uv-tools").join("bin"))),
        ("PIP_CACHE_DIR", p(cache.join("pip"))),
        ("npm_config_cache", p(cache.join("npm"))),
        ("npm_config_prefix", p(l.npm_global())),
        ("GIT_CONFIG_GLOBAL", p(l.gitconfig())),
    ] {
        env.set.push((k.into(), v));
    }
    for (k, v) in [
        // uv: use only the app's Python, copy instead of linking (the app may be on a USB drive),
        // and trust the Windows certificate store (school HTTPS inspection).
        ("UV_PYTHON_PREFERENCE", "only-managed"),
        // Only the app's own Python: no surprise downloads, no registry entries.
        ("UV_PYTHON_DOWNLOADS", "never"),
        ("UV_PYTHON_INSTALL_REGISTRY", "0"),
        ("UV_LINK_MODE", "copy"),
        ("UV_SYSTEM_CERTS", "1"),
        ("UV_NO_PROGRESS", "1"),
        ("PYTHONUTF8", "1"),
        ("NODE_USE_SYSTEM_CA", "1"),
        ("npm_config_update_notifier", "false"),
        ("npm_config_fund", "false"),
        ("npm_config_audit", "false"),
        ("POWERSHELL_TELEMETRY_OPTOUT", "1"),
        ("POWERSHELL_UPDATECHECK", "Off"),
        // Lets npm's PowerShell shims run where the default policy is Restricted; school policy
        // set by Group Policy still wins.
        ("PSExecutionPolicyPreference", "RemoteSigned"),
    ] {
        env.set.push((k.into(), v.into()));
    }
    let _ = ensure_gitconfig(l);
    env
}

/// A Git config inside the app folder, so `git config --global` works even when the profile is locked.
fn ensure_gitconfig(l: &Layout) -> std::io::Result<()> {
    let path = l.gitconfig();
    if !path.exists() {
        std::fs::create_dir_all(&l.data)?;
        // safe.directory avoids "dubious ownership" errors on USB drives and network shares.
        std::fs::write(&path, "[init]\n\tdefaultBranch = main\n[safe]\n\tdirectory = *\n[core]\n\tautocrlf = true\n")?;
    }
    Ok(())
}

/// Instructions given to both AIs so they use the portable tools instead of trying to install.
pub fn agent_notes(l: &Layout) -> String {
    let mut lines = vec![
        "## Tools on this school computer (provided by RustPortableHarness)".to_string(),
        "Nothing can be installed on this computer (no installers, no admin rights, no winget/choco/pip --user). Use only these tools; they are already on PATH:".into(),
    ];
    if usable(l, "python") {
        lines.push(format!("- Python {PYTHON_VERSION} through `uv` (tkinter included; it is the only Python version available, so keep `requires-python` compatible with {PYTHON_VERSION}). In a project: `uv run main.py`, add packages with `uv add <name>`. Create new environments with `uv venv` or `uv init`, never with system Python."));
    }
    if usable(l, "rust") {
        let window = if status(l, "rust").0 == "ready" { " Window apps: eframe/egui with `default-features = false, features = [\"default_fonts\", \"glow\"]`." } else { "" };
        lines.push(format!("- Rust (`cargo`, toolchain {RUST_TOOLCHAIN}, no Visual Studio). Use `cargo run`, `cargo add <crate>`. Avoid crates that need a C/C++ compiler, CMake or Visual Studio.{window}"));
    }
    if usable(l, "git") {
        lines.push("- Git (`git`). Set a local identity in each repo if a commit needs one.".into());
    }
    if usable(l, "node") {
        lines.push("- Node.js with `npm` and `npx`. Convex: `npx convex dev` (first run signs in, interactive), `npx convex deploy` (non-interactive when CONVEX_DEPLOY_KEY is in .env.local).".into());
    }
    if usable(l, "pwsh") {
        lines.push("- PowerShell 7 (`pwsh`).".into());
    }
    lines.push("Keep all files inside the project folder. If a tool you need is missing, tell the student to open Programming tools in the app rather than installing anything.".into());
    lines.join("\n")
}

const NOTES_START: &str = "<!-- RustPortableHarness:tools:start -->";
const NOTES_END: &str = "<!-- RustPortableHarness:tools:end -->";
/// Keeps a managed block in an AGENTS.md (Codex reads `$CODEX_HOME/AGENTS.md`), leaving other text alone.
pub fn write_notes_block(file: &Path, notes: &str) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(file).unwrap_or_default();
    let block = format!("{NOTES_START}\n{notes}\n{NOTES_END}");
    let next = match (existing.find(NOTES_START), existing.find(NOTES_END)) {
        (Some(a), Some(b)) if b > a => format!("{}{}{}", &existing[..a], block, &existing[b + NOTES_END.len()..]),
        _ if existing.trim().is_empty() => format!("{block}\n"),
        _ => format!("{}\n\n{block}\n", existing.trim_end()),
    };
    if next != existing {
        std::fs::write(file, next)?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Installing

pub type Report = Arc<dyn Fn(Option<u8>, &str, Option<&str>) + Send + Sync>;

fn hidden(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    #[cfg(not(windows))]
    let _ = cmd;
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn curl_error(code: Option<i32>, stderr: &str) -> String {
    let why = match code {
        Some(6) => "the web address could not be found (no internet, or the school network blocks it)",
        Some(7) => "could not connect (no internet, firewall or proxy)",
        Some(28) => "the download timed out",
        Some(35) | Some(60) => "the secure connection failed (school HTTPS inspection or a wrong PC clock)",
        Some(22) => "the server refused the download",
        Some(23) => "the file could not be saved (disk full or folder not writable)",
        _ => "the download did not finish",
    };
    let detail = stderr.trim().lines().last().unwrap_or("").trim();
    if detail.is_empty() { why.to_string() } else { format!("{why} ({detail})") }
}

/// Downloads with Windows' own curl.exe: it follows the proxy settings the app passes and trusts the
/// Windows certificate store, so it works behind school HTTPS inspection.
async fn download(l: &Layout, d: &Download, net: &ChildEnv, label: &str, report: &Report) -> Result<PathBuf, String> {
    let dir = l.downloads();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let dest = dir.join(d.file);
    if dest.is_file() {
        let (d2, sha) = (dest.clone(), d.sha256);
        if tokio::task::spawn_blocking(move || sha256_file(&d2)).await.map_err(|e| e.to_string())?? == sha {
            return Ok(dest);
        }
        let _ = std::fs::remove_file(&dest);
    }
    if let Some(free) = crate::netenv::free_space(&l.root) {
        if free < d.size + 200 * 1024 * 1024 {
            return Err(format!("Not enough free space for {label} ({} MB needed).", (d.size >> 20) + 200));
        }
    }
    let part = dir.join(format!("{}.part", d.file));
    let _ = std::fs::remove_file(&part);
    let curl = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("curl.exe"))
        .filter(|p| p.is_file())
        .ok_or("Windows' curl.exe is missing, so the app cannot download tools. Ask IT, or copy a prepared tools folder from your teacher.")?;
    let mut cmd = tokio::process::Command::new(curl);
    cmd.args(["-fL", "--ssl-revoke-best-effort", "--retry", "2", "--connect-timeout", "30", "-sS", "-o"])
        .arg(&part)
        .arg(d.url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    net.apply(&mut cmd);
    hidden(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Cannot start the download: {e}"))?;
    let mut stderr = child.stderr.take();
    let status = loop {
        tokio::select! {
            s = child.wait() => break s.map_err(|e| e.to_string())?,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                let pct = ((got * 100) / d.size.max(1)).min(99) as u8;
                report(Some(pct), &format!("Downloading {label}: {} of {} MB", got >> 20, d.size >> 20), None);
            }
        }
    };
    let mut err = String::new();
    if let Some(mut s) = stderr.take() {
        use tokio::io::AsyncReadExt;
        let _ = s.read_to_string(&mut err).await;
    }
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Could not download {label}: {}.", curl_error(status.code(), &err)));
    }
    report(Some(99), &format!("Checking {label}…"), None);
    let (p2, sha) = (part.clone(), d.sha256);
    let got = tokio::task::spawn_blocking(move || sha256_file(&p2)).await.map_err(|e| e.to_string())??;
    if got != sha {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{label} did not match its official checksum, so it was deleted. The network may have changed the file; try again or ask IT."));
    }
    rename_with_retry(&part, &dest).await?;
    Ok(dest)
}

/// Antivirus often scans a file right after it is written, and Windows then refuses to rename it
/// for a moment ("Access is denied"). Waiting and trying again fixes that.
async fn rename_with_retry(from: &Path, to: &Path) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..8u64 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(400 * (attempt + 1))).await;
    }
    Err(format!("Windows kept the file locked ({last}). Antivirus may still be scanning it; try again in a minute."))
}

/// Unpacks a zip. `strip_top` drops a single top folder; `only` keeps just those file names from `prefix`.
pub fn extract_zip(zip_path: &Path, dest: &Path, strip_top: bool, only: Option<(&str, &[&str])>) -> Result<usize, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("Damaged archive: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name rejects absolute paths and `..` (zip-slip).
        let Some(name) = entry.enclosed_name() else { continue };
        let mut rel: PathBuf = name.components().collect();
        if let Some((prefix, files)) = only {
            let Ok(rest) = rel.strip_prefix(prefix) else { continue };
            let file_name = rest.to_string_lossy().to_string();
            if !files.iter().any(|f| f.eq_ignore_ascii_case(&file_name)) {
                continue;
            }
            rel = PathBuf::from(file_name);
        } else if strip_top {
            let mut parts = rel.components();
            parts.next();
            rel = parts.collect();
            if rel.as_os_str().is_empty() {
                continue;
            }
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| format!("Cannot write {}: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

/// Runs a setup program hidden, passing its output lines to the progress log.
pub async fn run_logged(program: &Path, args: &[&str], cwd: &Path, env: &ChildEnv, report: &Report, message: &str) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    env.apply(&mut cmd);
    hidden(&mut cmd);
    let name = program.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut child = cmd.spawn().map_err(|e| format!("Cannot start {name}: {e}. Windows or school policy may be blocking it."))?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let tail = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let pump = |pipe: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>, tail: Arc<std::sync::Mutex<Vec<String>>>, report: Report, message: String| {
        tokio::spawn(async move {
            let Some(pipe) = pipe else { return };
            let mut lines = tokio::io::BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = crate::claude::strip_ansi(&line).trim().to_string();
                if line.is_empty() {
                    continue;
                }
                {
                    let mut t = tail.lock().unwrap();
                    t.push(line.clone());
                    if t.len() > 12 {
                        t.remove(0);
                    }
                }
                report(None, &message, Some(&line));
            }
        })
    };
    let a = pump(out.map(|o| Box::new(o) as _), tail.clone(), report.clone(), message.to_string());
    let b = pump(err.map(|o| Box::new(o) as _), tail.clone(), report.clone(), message.to_string());
    report(None, message, None);
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = tokio::time::timeout(Duration::from_secs(2), a).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), b).await;
    if status.success() {
        Ok(())
    } else {
        let last = tail.lock().unwrap().iter().rev().take(4).rev().cloned().collect::<Vec<_>>().join("\n");
        Err(format!("{name} failed: {last}"))
    }
}

fn remove_inside(path: &Path, parent: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&parent) || canonical == parent || std::fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(true) {
        return Err("Refusing to remove a folder outside the app's tools folder.".into());
    }
    std::fs::remove_dir_all(path).map_err(|e| format!("Could not remove {} ({e}). Close programs that use it and try again.", path.display()))
}

/// Downloads and sets up one tool inside the app folder.
pub async fn install(l: &Layout, id: &str, net: &ChildEnv, report: &Report) -> Result<(), String> {
    std::fs::create_dir_all(l.tools()).map_err(|e| format!("Cannot create the tools folder: {e}"))?;
    let tools = l.tools();
    let mut env = net.clone();
    env.extend(&self::env(l));
    match id {
        "python" => {
            let zip = download(l, &UV, net, "uv", report).await?;
            remove_inside(&l.uv_dir(), &tools)?;
            let (z, d) = (zip.clone(), l.uv_dir());
            tokio::task::spawn_blocking(move || extract_zip(&z, &d, false, None)).await.map_err(|e| e.to_string())??;
            let archive = download(l, &PYTHON, net, &format!("Python {PYTHON_VERSION}"), report).await?;
            remove_inside(&l.python_home(), &tools)?;
            let dir = l.python_home().join(PYTHON_DIR);
            std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create the Python folder: {e}"))?;
            let tar = std::env::var_os("SystemRoot")
                .map(|r| PathBuf::from(r).join("System32").join("tar.exe"))
                .filter(|p| p.is_file())
                .ok_or("Windows' tar.exe is missing, so Python cannot be unpacked. Ask IT, or use a classroom copy that already has the tools.")?;
            let (a, d) = (display_path(&archive), display_path(&dir));
            run_logged(&tar, &["-xzf", &a, "-C", &d, "--strip-components=1"], &l.data, net, report, &format!("Unpacking Python {PYTHON_VERSION}…")).await?;
            let python = dir.join("python.exe");
            run_logged(&python, &["-c", "import tkinter, sys; print('Python', sys.version.split()[0], 'with tkinter', tkinter.TkVersion)"], &l.data, net, report, "Checking Python and tkinter…").await?;
            let _ = std::fs::remove_file(zip);
            let _ = std::fs::remove_file(archive);
        }
        "rust" => {
            if l.rust_path_too_long() {
                return Err("The app folder's path is too long for Rust. Move the RustPortableHarness folder closer to the top of the drive (for example Desktop\\Harness), then try again.".into());
            }
            let init = download(l, &RUSTUP, net, "the Rust installer", report).await?;
            run_logged(
                &init,
                &["-y", "--no-modify-path", "--profile", "minimal", "--default-host", "x86_64-pc-windows-gnu", "--default-toolchain", "stable"],
                &l.data,
                &env,
                report,
                "Installing Rust (about 100 MB; this takes a few minutes)…",
            )
            .await?;
            let zip = download(l, &WINLIBS, net, "Rust's Windows helpers (MinGW binutils)", report).await?;
            report(None, "Unpacking Rust's Windows helpers…", None);
            let (z, d) = (zip.clone(), l.binutils());
            let n = tokio::task::spawn_blocking(move || extract_zip(&z, &d, false, Some(("mingw64/bin", &BINUTILS)))).await.map_err(|e| e.to_string())??;
            if n < BINUTILS.len() {
                return Err("Rust's Windows helpers were incomplete. Try again.".into());
            }
            let _ = std::fs::remove_file(zip);
            let _ = std::fs::remove_file(init);
        }
        "git" => {
            let sfx = download(l, &GIT, net, "Git", report).await?;
            remove_inside(&l.git_dir(), &tools)?;
            let out = format!("-o{}", display_path(&l.git_dir()));
            run_logged(&sfx, &[&out, "-y"], &l.data, net, report, "Unpacking Git…").await?;
            if !l.git_exe().is_file() {
                return Err("Git did not unpack correctly. Antivirus may have blocked it.".into());
            }
            let _ = std::fs::remove_file(sfx);
        }
        "pwsh" => {
            let zip = download(l, &PWSH, net, "PowerShell", report).await?;
            remove_inside(&l.pwsh_dir(), &tools)?;
            report(None, "Unpacking PowerShell…", None);
            let (z, d) = (zip.clone(), l.pwsh_dir());
            tokio::task::spawn_blocking(move || extract_zip(&z, &d, false, None)).await.map_err(|e| e.to_string())??;
            let _ = std::fs::remove_file(zip);
        }
        "node" => {
            let zip = download(l, &NODE, net, "Node.js", report).await?;
            remove_inside(&l.node_dir(), &tools)?;
            report(None, "Unpacking Node.js…", None);
            let (z, d) = (zip.clone(), l.node_dir());
            tokio::task::spawn_blocking(move || extract_zip(&z, &d, true, None)).await.map_err(|e| e.to_string())??;
            std::fs::create_dir_all(l.npm_global()).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(zip);
        }
        _ => return Err("Unknown tool".into()),
    }
    let (state, detail) = status(l, id);
    if state == "ready" {
        Ok(())
    } else {
        Err(format!("Setup finished but the tool is not ready: {detail}"))
    }
}

pub fn remove(l: &Layout, id: &str) -> Result<(), String> {
    let tools = l.tools();
    match id {
        "python" => {
            remove_inside(&l.uv_dir(), &tools)?;
            remove_inside(&l.python_home(), &tools)
        }
        "rust" => remove_inside(&l.rust_dir(), &tools),
        "git" => remove_inside(&l.git_dir(), &tools),
        "pwsh" => remove_inside(&l.pwsh_dir(), &tools),
        "node" => remove_inside(&l.node_dir(), &tools),
        _ => Err("Unknown tool".into()),
    }
}

// ------------------------------------------------------------------------------------------------
// Run and Terminal

/// How to start the selected project, if the app recognises it.
pub fn runner(project: &Path) -> Option<(String, String, &'static str)> {
    if project.join("Cargo.toml").is_file() {
        return Some(("cargo run".into(), "cargo run".into(), "rust"));
    }
    if project.join("package.json").is_file() {
        let pkg: Value = std::fs::read_to_string(project.join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        for script in ["dev", "start"] {
            if pkg["scripts"][script].is_string() {
                let label = if script == "start" { "npm start".to_string() } else { format!("npm run {script}") };
                return Some((label.clone(), label, "node"));
            }
        }
        return None;
    }
    for file in ["main.py", "app.py"] {
        if project.join(file).is_file() {
            return Some((format!("uv run {file}"), format!("uv run {file}"), "python"));
        }
    }
    None
}

pub fn project_info(l: &Layout, project: Option<&str>) -> Value {
    let Some(p) = project.map(Path::new).filter(|p| p.is_dir()) else {
        return Value::Null;
    };
    match runner(p) {
        Some((label, _, tool)) => json!({"run": label, "tool": tool, "toolReady": usable(l, tool)}),
        None => json!({"run": null}),
    }
}

/// The best shell for a visible console: the app's PowerShell 7, then the PC's, then cmd.
fn console_shell(l: &Layout) -> (PathBuf, bool) {
    if l.pwsh().is_file() {
        return (l.pwsh(), true);
    }
    if let Some(p) = on_path("pwsh.exe") {
        return (p, true);
    }
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let ps = root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if ps.is_file() {
        return (ps, true);
    }
    (root.join(r"System32\cmd.exe"), false)
}

fn plain(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric() || " -_.()".contains(*c)).take(60).collect()
}

/// Opens a normal console window in the project folder with every tool on PATH, optionally running
/// a command (the window stays open so students can read the output and type input).
pub fn open_console(l: &Layout, project: &Path, title: &str, command: Option<&str>, env: &ChildEnv) -> Result<(), String> {
    let (shell, powershell) = console_shell(l);
    let title = plain(title);
    let mut tools: Vec<&str> = vec![];
    for (id, name) in [("python", "python/uv"), ("rust", "cargo"), ("git", "git"), ("node", "node/npm/npx"), ("pwsh", "pwsh")] {
        if usable(l, id) {
            tools.push(name);
        }
    }
    let banner = if tools.is_empty() { "No programming tools set up yet (see Programming tools in the app)".to_string() } else { format!("Tools ready: {}", tools.join(", ")) };
    let mut cmd = std::process::Command::new(&shell);
    if powershell {
        let mut script = format!("$Host.UI.RawUI.WindowTitle = '{title}'; Write-Host '{banner}' -ForegroundColor DarkCyan");
        if let Some(c) = command {
            script.push_str(&format!("; Write-Host '> {c}' -ForegroundColor Cyan; {c}"));
        }
        cmd.args(["-NoLogo", "-NoExit", "-ExecutionPolicy", "RemoteSigned", "-Command", &script]);
    } else {
        let mut line = format!("title {title} & echo {banner}");
        if let Some(c) = command {
            line.push_str(&format!(" & echo ^> {c} & {c}"));
        }
        cmd.args(["/k", &line]);
    }
    cmd.current_dir(project);
    for k in &env.remove {
        cmd.env_remove(k);
    }
    for (k, v) in &env.set {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x00000010); // CREATE_NEW_CONSOLE
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Could not open a console ({}): {e}. School policy may block command windows.", shell.file_name().unwrap_or_default().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_block_is_replaced_not_duplicated() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("AGENTS.md");
        std::fs::write(&f, "# My rules\nBe kind.\n").unwrap();
        write_notes_block(&f, "first").unwrap();
        write_notes_block(&f, "second").unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.starts_with("# My rules"));
        assert!(text.contains("second") && !text.contains("first"));
        assert_eq!(text.matches(NOTES_START).count(), 1);
    }
    #[test]
    fn runner_detects_project_kinds() {
        let t = tempfile::tempdir().unwrap();
        assert!(runner(t.path()).is_none());
        std::fs::write(t.path().join("main.py"), "print(1)").unwrap();
        assert_eq!(runner(t.path()).unwrap().0, "uv run main.py");
        std::fs::write(t.path().join("package.json"), r#"{"scripts":{"dev":"convex dev"}}"#).unwrap();
        assert_eq!(runner(t.path()).unwrap().0, "npm run dev");
        std::fs::write(t.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(runner(t.path()).unwrap().2, "rust");
    }
    #[test]
    fn env_points_everything_into_the_app_folder() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path(), &t.path().join("data"));
        std::fs::create_dir_all(l.uv_dir()).unwrap();
        std::fs::write(l.uv(), b"").unwrap();
        let env = env(&l);
        let get = |k: &str| env.set.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
        assert!(get("CARGO_HOME").starts_with(&display_path(t.path())));
        assert!(get("UV_PYTHON_BIN_DIR").contains("tools"));
        assert!(get("PATH").starts_with(&display_path(&l.uv_dir())));
        assert!(l.gitconfig().is_file());
        assert_eq!(status(&l, "python").0, "partial");
    }
    #[test]
    fn zip_extraction_strips_top_folder_and_filters() {
        let t = tempfile::tempdir().unwrap();
        let zpath = t.path().join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zpath).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for name in ["top/bin/as.exe", "top/bin/other.exe", "top/readme.txt", "../evil.txt"] {
                w.start_file(name, o).unwrap();
                w.write_all(b"x").unwrap();
            }
            w.finish().unwrap();
        }
        let out = t.path().join("out");
        assert_eq!(extract_zip(&zpath, &out, true, None).unwrap(), 3);
        assert!(out.join("bin/as.exe").is_file() && out.join("readme.txt").is_file());
        assert!(!t.path().join("evil.txt").exists());
        let only = t.path().join("only");
        assert_eq!(extract_zip(&zpath, &only, false, Some(("top/bin", &["as.exe"]))).unwrap(), 1);
        assert!(only.join("as.exe").is_file());
    }
}
