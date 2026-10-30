//! Claude engine.
//!
//! Runs the official Claude Code CLI (`claude.exe`) in its documented headless mode
//! (`-p --input-format stream-json --output-format stream-json`) and maps its events onto the same
//! item model the Codex view already renders. The CLI stays the agent, the sign-in client and the
//! permission authority: this module only relays prompts, permission answers and interrupts.
//! Each account uses its own `CLAUDE_CONFIG_DIR` inside the portable data folder.
use crate::{
    netenv::ChildEnv,
    portable_paths::Config,
    state::{append, Approval, BinaryInfo, Runtime},
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::mpsc,
};
use tokio_util::codec::{FramedRead, LinesCodec};

pub type Handler = Arc<dyn Fn(Value) + Send + Sync>;
const MAX_FRAME: usize = 8 * 1024 * 1024;
/// Oldest Claude Code with the stream-json control channel, rate-limit events and `auth status --json`.
pub const MIN_VERSION: [u32; 3] = [2, 1, 0];
pub const APPROVAL_METHOD: &str = "claude/canUseTool";

fn hidden(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    #[cfg(not(windows))]
    let _ = cmd;
}

/// One running `claude.exe` process bound to one conversation.
#[derive(Clone)]
pub struct Session {
    tx: mpsc::Sender<String>,
    stop: mpsc::Sender<()>,
    exited: Arc<AtomicBool>,
    next: Arc<AtomicU64>,
    pub id: String,
    /// Settings the process was started with; a change restarts it with `--resume`.
    pub signature: String,
}

pub struct Launch<'a> {
    pub binary: &'a Path,
    pub cwd: &'a Path,
    pub env: &'a ChildEnv,
    pub session_id: &'a str,
    pub resume: bool,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub permission_mode: &'a str,
    pub project_settings: bool,
    /// Extra instructions (which portable tools exist on this computer).
    pub system_note: Option<&'a str>,
}
impl Launch<'_> {
    pub fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompt-tool",
            "stdio",
            "--permission-mode",
            self.permission_mode,
            // Project settings files can define hooks that run commands; only the account's own
            // settings are read unless a teacher turns project settings on.
            "--setting-sources",
            if self.project_settings {
                "user,project,local"
            } else {
                "user"
            },
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if let Some(model) = self.model {
            args.extend(["--model".into(), model.into()]);
        }
        if let Some(effort) = self.effort {
            args.extend(["--effort".into(), effort.into()]);
        }
        if let Some(note) = self.system_note.filter(|n| !n.trim().is_empty()) {
            args.extend(["--append-system-prompt".into(), note.into()]);
        }
        let flag = if self.resume { "--resume" } else { "--session-id" };
        args.extend([flag.into(), self.session_id.into()]);
        args
    }
}

impl Session {
    pub async fn spawn(o: Launch<'_>, signature: String, handler: Handler) -> Result<Self, String> {
        let mut cmd = Command::new(o.binary);
        cmd.args(o.args())
            .current_dir(o.cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        o.env.apply(&mut cmd);
        hidden(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| {
            format!("Cannot start claude.exe: {e}. If Windows blocked it, ask IT to allow the app folder; if it is missing, extract the ZIP again.")
        })?;
        #[cfg(windows)]
        let job = crate::rpc_transport::Job::assign(&child)?;
        let mut stdin = child.stdin.take().ok_or("Claude input unavailable")?;
        let stdout = child.stdout.take().ok_or("Claude output unavailable")?;
        let stderr = child.stderr.take().ok_or("Claude error output unavailable")?;
        let (tx, mut rx) = mpsc::channel::<String>(128);
        let (stop, mut stoprx) = mpsc::channel::<()>(1);
        let exited = Arc::new(AtomicBool::new(false));
        let session = Self {
            tx,
            stop,
            exited: exited.clone(),
            next: Arc::new(AtomicU64::new(1)),
            id: o.session_id.to_string(),
            signature,
        };
        let kill = session.stop.clone();
        tokio::spawn(async move {
            while let Some(mut line) = rx.recv().await {
                line.push('\n');
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err()
                {
                    let _ = kill.send(()).await;
                    break;
                }
            }
        });
        let h = handler.clone();
        let kill = session.stop.clone();
        let reader = tokio::spawn(async move {
            let mut lines = FramedRead::new(stdout, LinesCodec::new_with_max_length(MAX_FRAME));
            while let Some(line) = lines.next().await {
                match line {
                    Ok(line) => {
                        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                            if v.is_object() {
                                h(v);
                            }
                        }
                    }
                    Err(_) => {
                        h(json!({"localError":"Claude sent an oversized or unreadable message (limit 8 MiB)."}));
                        let _ = kill.send(()).await;
                        break;
                    }
                }
            }
        });
        let tail = Arc::new(Mutex::new(String::new()));
        let t = tail.clone();
        tokio::spawn(async move { keep_tail(stderr, t).await });
        tokio::spawn(async move {
            let status = tokio::select! {
                r = child.wait() => r,
                _ = stoprx.recv() => { let _ = child.kill().await; child.wait().await }
            };
            #[cfg(windows)]
            drop(job);
            // Let the reader deliver the last lines (usually the final result) before the exit.
            let _ = tokio::time::timeout(Duration::from_secs(3), reader).await;
            exited.store(true, Ordering::Release);
            let text = strip_ansi(&tail.lock().unwrap());
            handler(json!({"localExit": status.ok().and_then(|s| s.code()), "stderr": last_lines(&text, 8)}));
        });
        session
            .send(json!({"type":"control_request","request_id":"init","request":{"subtype":"initialize"}}))
            .await?;
        Ok(session)
    }
    pub async fn send(&self, v: Value) -> Result<(), String> {
        self.tx
            .send(v.to_string())
            .await
            .map_err(|_| "Claude stopped. Send your message again to restart it.".to_string())
    }
    pub async fn user_message(&self, text: &str) -> Result<(), String> {
        self.send(json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":text}]},"parent_tool_use_id":null,"session_id":""}))
            .await
    }
    pub async fn interrupt(&self) -> Result<(), String> {
        let id = format!("interrupt-{}", self.next.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type":"control_request","request_id":id,"request":{"subtype":"interrupt"}}))
            .await
    }
    pub fn alive(&self) -> bool {
        !self.exited.load(Ordering::Acquire)
    }
    pub fn stop_now(&self) {
        let _ = self.stop.try_send(());
    }
    pub async fn stop(&self) {
        let _ = self.stop.send(()).await;
        for _ in 0..60 {
            if self.exited.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

async fn keep_tail(mut pipe: impl tokio::io::AsyncRead + Unpin, tail: Arc<Mutex<String>>) {
    let mut buf = [0u8; 4096];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut s = tail.lock().unwrap();
                s.push_str(&String::from_utf8_lossy(&buf[..n]));
                if s.len() > 8192 {
                    let mut cut = s.len() - 8192;
                    while !s.is_char_boundary(cut) {
                        cut += 1;
                    }
                    s.drain(..cut);
                }
            }
        }
    }
}

/// Removes terminal colour codes and hyperlinks so messages and URLs are plain text.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else if c == '\r' || (c.is_control() && c != '\n' && c != '\t') {
            continue;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    let joined = lines[start..].join("\n");
    joined.chars().take(1200).collect()
}

pub fn find_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let url: String = text[start..]
        .chars()
        .take_while(|c| !c.is_whitespace() && !c.is_control() && *c != '"' && *c != '\'')
        .collect();
    url::Url::parse(&url).ok().map(|_| url)
}

pub fn parse_version(text: &str) -> Option<[u32; 3]> {
    let token = text.split_whitespace().next()?;
    let mut parts = token.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .ok()
    });
    Some([parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)])
}

/// Where to look for Claude: an explicit choice, the app folder, then the official per-user install.
pub fn find_binary(root: &Path, config: &Config) -> Option<PathBuf> {
    if let Some(path) = config.claude_binary_path.as_deref() {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }
    [root.join("claude.exe"), root.join("claude").join("claude.exe")]
        .into_iter()
        .find(|p| p.is_file())
        .or_else(crate::netenv::user_claude_install)
}

pub async fn validate(path: PathBuf) -> Result<BinaryInfo, String> {
    let p = path.clone();
    let hash = tokio::task::spawn_blocking(move || crate::commands::hash_executable(&p, "claude.exe"))
        .await
        .map_err(|e| e.to_string())??;
    let mut c = Command::new(&path);
    c.arg("--version")
        .stdin(std::process::Stdio::null())
        .env_remove("CLAUDECODE")
        .env("DISABLE_AUTOUPDATER", "1")
        .kill_on_drop(true);
    hidden(&mut c);
    // Antivirus scans a large new executable on its first run; allow a generous first start.
    let out = tokio::time::timeout(Duration::from_secs(90), c.output())
        .await
        .map_err(|_| "claude.exe did not answer. Windows may still be scanning it (the first start can take a minute). Try again shortly.".to_string())?
        .map_err(|e| format!("Cannot run claude.exe: {e}. Windows or school policy may be blocking it."))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || !text.contains("Claude Code") {
        return Err(format!(
            "This file does not look like the official Claude Code program (it answered \"{}\"). Choose claude.exe from Anthropic.",
            text.chars().take(80).collect::<String>()
        ));
    }
    let version = parse_version(&text).ok_or("Could not read the Claude Code version")?;
    if version < MIN_VERSION {
        return Err(format!(
            "This claude.exe is version {text}. RustPortableHarness needs Claude Code {}.{}.{} or newer.",
            MIN_VERSION[0], MIN_VERSION[1], MIN_VERSION[2]
        ));
    }
    Ok(BinaryInfo {
        path: crate::portable_paths::display_path(&path),
        version: text,
        sha256: hash,
    })
}

async fn run_cli(
    binary: &Path,
    env: &ChildEnv,
    cwd: &Path,
    args: &[&str],
    secs: u64,
) -> Result<(Option<i32>, String, String), String> {
    let mut c = Command::new(binary);
    c.args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    env.apply(&mut c);
    hidden(&mut c);
    let out = tokio::time::timeout(Duration::from_secs(secs), c.output())
        .await
        .map_err(|_| "Claude did not answer in time. Check the network or proxy, then try again.".to_string())?
        .map_err(|e| format!("Cannot run claude.exe: {e}"))?;
    Ok((
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    ))
}

pub async fn auth_status(binary: &Path, env: &ChildEnv, cwd: &Path) -> Result<Value, String> {
    let (_, out, err) = run_cli(binary, env, cwd, &["auth", "status", "--json"], 60).await?;
    serde_json::from_str::<Value>(out.trim()).map_err(|_| {
        format!(
            "Could not read the Claude sign-in status. {}",
            last_lines(&strip_ansi(&err), 4)
        )
    })
}

pub async fn logout(binary: &Path, env: &ChildEnv, cwd: &Path) -> Result<(), String> {
    let (code, _, err) = run_cli(binary, env, cwd, &["auth", "logout"], 60).await?;
    if code == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "Claude sign-out did not finish. {}",
            last_lines(&strip_ansi(&err), 4)
        ))
    }
}

/// `{account:{type,email,planType,org}}` in the shape the account UI already reads.
pub fn account_from_status(v: &Value) -> Value {
    if v["loggedIn"] != true {
        return json!({"account": null, "loggedIn": false});
    }
    let email = v["email"].as_str();
    let plan = v["subscriptionType"]
        .as_str()
        .or_else(|| v["authMethod"].as_str());
    json!({"loggedIn": true, "account": {"type": "claude", "email": email, "planType": plan, "org": v["orgName"], "authMethod": v["authMethod"]}})
}

/// A running `claude auth login` (the official browser sign-in).
pub struct Login {
    stdin: mpsc::Sender<String>,
    stop: mpsc::Sender<()>,
}
impl Login {
    pub async fn start(
        binary: &Path,
        env: &ChildEnv,
        cwd: &Path,
        console: bool,
        handler: Handler,
    ) -> Result<Self, String> {
        let mut c = Command::new(binary);
        c.args([
            "auth",
            "login",
            if console { "--console" } else { "--claudeai" },
        ])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
        env.apply(&mut c);
        hidden(&mut c);
        let mut child = c
            .spawn()
            .map_err(|e| format!("Cannot start Claude sign-in: {e}"))?;
        #[cfg(windows)]
        let job = crate::rpc_transport::Job::assign(&child)?;
        let mut stdin = child.stdin.take().ok_or("Sign-in input unavailable")?;
        let stdout = child.stdout.take().ok_or("Sign-in output unavailable")?;
        let stderr = child.stderr.take().ok_or("Sign-in output unavailable")?;
        let (tx, mut rx) = mpsc::channel::<String>(4);
        let (stop, mut stoprx) = mpsc::channel::<()>(1);
        tokio::spawn(async move {
            while let Some(mut line) = rx.recv().await {
                line.push('\n');
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });
        let output = Arc::new(Mutex::new(String::new()));
        let watch = |pipe: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
                     output: Arc<Mutex<String>>,
                     handler: Handler| {
            tokio::spawn(async move {
                let mut pipe = pipe;
                let mut buf = [0u8; 2048];
                loop {
                    match pipe.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let text = {
                                let mut o = output.lock().unwrap();
                                o.push_str(&String::from_utf8_lossy(&buf[..n]));
                                if o.len() > 16384 {
                                    let mut cut = o.len() - 16384;
                                    while !o.is_char_boundary(cut) {
                                        cut += 1;
                                    }
                                    o.drain(..cut);
                                }
                                strip_ansi(&o)
                            };
                            handler(json!({"loginUpdate": true, "url": find_url(&text), "output": last_lines(&text, 6)}));
                        }
                    }
                }
            })
        };
        let a = watch(Box::new(stdout), output.clone(), handler.clone());
        let b = watch(Box::new(stderr), output.clone(), handler.clone());
        tokio::spawn(async move {
            let status = tokio::select! {
                r = child.wait() => r.ok().and_then(|s| s.code()),
                _ = stoprx.recv() => { let _ = child.kill().await; let _ = child.wait().await; None }
                _ = tokio::time::sleep(Duration::from_secs(15 * 60)) => { let _ = child.kill().await; let _ = child.wait().await; None }
            };
            #[cfg(windows)]
            drop(job);
            let _ = tokio::time::timeout(Duration::from_secs(2), a).await;
            let _ = tokio::time::timeout(Duration::from_secs(2), b).await;
            let text = strip_ansi(&output.lock().unwrap());
            handler(json!({"loginExit": status, "output": last_lines(&text, 6)}));
        });
        Ok(Self { stdin: tx, stop })
    }
    /// Where a code pasted from the browser is written, for the fallback sign-in path.
    pub fn stdin_sender(&self) -> mpsc::Sender<String> {
        self.stdin.clone()
    }
    pub fn cancel(&self) {
        let _ = self.stop.try_send(());
    }
}

// ---------------------------------------------------------------------------------------------
// Conversations on disk (`<CLAUDE_CONFIG_DIR>/projects/<folder name>/<session id>.jsonl`).

/// Claude names a project's folder by replacing every character except A–Z, a–z, 0–9 with '-'.
pub fn sanitize_cwd(cwd: &str) -> String {
    cwd.encode_utf16()
        .map(|u| {
            if u < 128 && (u as u8).is_ascii_alphanumeric() {
                u as u8 as char
            } else {
                '-'
            }
        })
        .collect()
}

fn project_dirs(home: &Path, cwd: &str) -> Vec<PathBuf> {
    let base = home.join("projects");
    let name = sanitize_cwd(cwd);
    let mut dirs = vec![];
    let exact = base.join(&name);
    if exact.is_dir() {
        dirs.push(exact);
    }
    // Very long paths are shortened and given a hash suffix.
    if name.len() > 200 {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n != name && n.starts_with(&name[..200]) && e.path().is_dir() {
                    dirs.push(e.path());
                }
            }
        }
    }
    dirs
}

pub fn valid_session_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn session_file(home: &Path, cwd: &str, id: &str) -> Option<PathBuf> {
    if !valid_session_id(id) {
        return None;
    }
    project_dirs(home, cwd)
        .into_iter()
        .map(|d| d.join(format!("{id}.jsonl")))
        .find(|p| p.is_file())
}

fn read_head(path: &Path, limit: usize) -> String {
    use std::io::Read;
    let mut buf = vec![];
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.take(limit as u64).read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn user_text(message: &Value) -> Option<String> {
    let text = match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let t = text.trim();
    if t.is_empty()
        || t.starts_with('<')
        || t.starts_with("Caveat:")
        || t.starts_with("[Request interrupted")
    {
        return None;
    }
    Some(t.to_string())
}

pub fn list_threads(home: &Path, cwd: &str) -> Vec<Value> {
    let mut threads = vec![];
    for dir in project_dirs(home, cwd) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|x| x.to_str()).map(str::to_string) else {
                continue;
            };
            if !valid_session_id(&id) {
                continue;
            }
            let updated = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let (mut title, mut preview) = (None, None);
            for line in read_head(&path, 512 * 1024).lines() {
                let Ok(v) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                match v["type"].as_str() {
                    Some("custom-title") => title = v["customTitle"].as_str().map(str::to_string),
                    Some("summary") if title.is_none() => {
                        title = v["summary"].as_str().map(str::to_string)
                    }
                    Some("user")
                        if preview.is_none() && v["isSidechain"] != true && v["isMeta"] != true =>
                    {
                        preview = user_text(&v["message"])
                    }
                    _ => {}
                }
            }
            let Some(name) = title.clone().or(preview.clone()) else {
                continue;
            };
            threads.push(json!({"id": id, "name": name.chars().take(120).collect::<String>(), "preview": preview, "updatedAt": updated}));
        }
    }
    threads.sort_by(|a, b| b["updatedAt"].as_u64().cmp(&a["updatedAt"].as_u64()));
    threads.truncate(200);
    threads
}

pub fn load_history(home: &Path, cwd: &str, id: &str) -> Result<Vec<Value>, String> {
    let path = session_file(home, cwd, id)
        .ok_or("This Claude conversation was not found for this project and account.")?;
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let limit = 16 * 1024 * 1024;
    let text = if size > limit {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(size - limit)).map_err(|e| e.to_string())?;
        let mut buf = vec![];
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        let s = String::from_utf8_lossy(&buf).to_string();
        s.split_once('\n').map(|x| x.1.to_string()).unwrap_or_default()
    } else {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    };
    let mut items = vec![];
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["isSidechain"] == true {
            continue;
        }
        match v["type"].as_str() {
            Some("user") => {
                if v["isMeta"] != true {
                    if let Some(t) = user_text(&v["message"]) {
                        let uid = v["uuid"].as_str().unwrap_or("user").to_string();
                        items.push(json!({"id": uid, "type": "userMessage", "content": [{"type":"text","text": t}]}));
                    }
                }
                apply_tool_results(&mut items, &v["message"]);
            }
            Some("assistant") => {
                if v["isApiErrorMessage"] == true {
                    continue;
                }
                apply_assistant(&mut items, &v["message"]);
            }
            _ => {}
        }
    }
    for item in items.iter_mut() {
        if item["status"] == "inProgress" {
            item["status"] = json!("completed");
        }
    }
    if items.len() > 1600 {
        items.drain(..items.len() - 1600);
    }
    Ok(items)
}

/// Moves a conversation file into `<account>/archived/` so Claude no longer lists or resumes it.
pub fn archive_thread(home: &Path, cwd: &str, id: &str) -> Result<(), String> {
    let path = session_file(home, cwd, id).ok_or("Conversation not found")?;
    let dest_dir = home.join("archived");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    std::fs::rename(&path, dest_dir.join(format!("{id}.jsonl"))).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Event mapping onto the shared item model.

fn upsert(items: &mut Vec<Value>, item: Value) {
    let Some(id) = item["id"].as_str().map(str::to_string) else {
        return;
    };
    if let Some(existing) = items.iter_mut().find(|i| i["id"] == id.as_str()) {
        if let (Some(old), Some(new)) = (existing.as_object_mut(), item.as_object()) {
            for (k, v) in new {
                old.insert(k.clone(), v.clone());
            }
        }
    } else {
        items.push(item);
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n[Output truncated]", &text[..cut])
}

fn synth_diff(path: &str, old: &str, new: &str) -> String {
    let mut out = format!("--- a/{path}\n+++ b/{path}\n@@ @@\n");
    for l in old.lines() {
        out.push('-');
        out.push_str(l);
        out.push('\n');
    }
    for l in new.lines() {
        out.push('+');
        out.push_str(l);
        out.push('\n');
    }
    truncate(&out, 256 * 1024)
}

/// Bash/PowerShell → command card, Edit/Write → file change with a readable patch, else a tool card.
pub fn tool_item(id: &str, name: &str, input: &Value) -> Value {
    match name {
        "Bash" | "PowerShell" => json!({
            "id": id, "type": "commandExecution", "tool": name, "status": "inProgress",
            "command": input["command"].as_str().unwrap_or(""),
            "description": input["description"],
        }),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            let path = input["file_path"]
                .as_str()
                .or_else(|| input["notebook_path"].as_str())
                .unwrap_or("file");
            let diff = match name {
                "Write" => synth_diff(path, "", input["content"].as_str().unwrap_or("")),
                "Edit" => synth_diff(
                    path,
                    input["old_string"].as_str().unwrap_or(""),
                    input["new_string"].as_str().unwrap_or(""),
                ),
                "MultiEdit" => input["edits"]
                    .as_array()
                    .map(|edits| {
                        edits
                            .iter()
                            .map(|e| {
                                synth_diff(
                                    path,
                                    e["old_string"].as_str().unwrap_or(""),
                                    e["new_string"].as_str().unwrap_or(""),
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default(),
                _ => synth_diff(path, "", input["new_source"].as_str().unwrap_or("")),
            };
            json!({
                "id": id, "type": "fileChange", "tool": name, "status": "inProgress",
                "changes": [{"path": path, "kind": {"type": if name == "Write" { "write" } else { "update" }}, "diff": diff}],
            })
        }
        _ => json!({"id": id, "type": "toolCall", "tool": name, "input": input, "status": "inProgress"}),
    }
}

fn final_text(items: &mut Vec<Value>, msg: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let prefix = format!("{msg}:");
    let owned = |i: &Value| {
        i["type"] == "agentMessage"
            && i["id"].as_str().map(|x| x.starts_with(&prefix)).unwrap_or(false)
    };
    if let Some(item) = items.iter_mut().find(|i| {
        owned(i) && i["final"] != true && text.starts_with(i["text"].as_str().unwrap_or(""))
    }) {
        item["text"] = json!(text);
        item["final"] = json!(true);
        return;
    }
    if items
        .iter()
        .any(|i| owned(i) && i["text"].as_str() == Some(text))
    {
        return;
    }
    let n = items.iter().filter(|i| owned(i)).count();
    items.push(json!({"id": format!("{prefix}f{n}"), "type": "agentMessage", "text": text, "final": true}));
}

pub fn apply_assistant(items: &mut Vec<Value>, message: &Value) {
    let msg = message["id"].as_str().unwrap_or("assistant").to_string();
    let Some(blocks) = message["content"].as_array() else {
        return;
    };
    for block in blocks {
        match block["type"].as_str() {
            Some("text") => final_text(items, &msg, block["text"].as_str().unwrap_or("")),
            Some("thinking") | Some("redacted_thinking") => upsert(
                items,
                json!({"id": format!("{msg}:thinking"), "type": "reasoning", "status": "completed"}),
            ),
            Some("tool_use") | Some("server_tool_use") => {
                let id = block["id"].as_str().unwrap_or("");
                if id.is_empty() {
                    continue;
                }
                let done = items
                    .iter()
                    .find(|i| i["id"] == id)
                    .map(|i| i["status"] != "inProgress")
                    .unwrap_or(false);
                let mut item = tool_item(id, block["name"].as_str().unwrap_or("tool"), &block["input"]);
                if done {
                    item.as_object_mut().map(|o| o.remove("status"));
                }
                upsert(items, item);
            }
            _ => {}
        }
    }
}

fn tool_result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p["type"].as_str() {
                Some("text") => p["text"].as_str().unwrap_or("").to_string(),
                Some("image") => "[image]".into(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn apply_tool_results(items: &mut [Value], message: &Value) {
    let Some(blocks) = message["content"].as_array() else {
        return;
    };
    for block in blocks.iter().filter(|b| b["type"] == "tool_result") {
        let id = block["tool_use_id"].as_str().unwrap_or("");
        if let Some(item) = items.iter_mut().find(|i| i["id"] == id) {
            let text = tool_result_text(&block["content"]);
            let error = block["is_error"] == true;
            let declined = error
                && (text.contains("declined") || text.contains("denied") || text.contains("rejected"));
            item["status"] = json!(if declined {
                "declined"
            } else if error {
                "failed"
            } else {
                "completed"
            });
            item["aggregatedOutput"] = json!(truncate(&text, 64 * 1024));
        }
    }
}

fn percent(v: &Value) -> Option<i64> {
    v.as_f64().map(|u| (u * 100.0).round().clamp(0.0, 999.0) as i64)
}

/// Claude reports usage as a 0–1 fraction per window; stored as percent like Codex.
pub fn usage_from_event(info: &Value) -> Value {
    let windows = &info["unifiedWindows"];
    let window = |key: &str, mins: u64| {
        let w = &windows[key];
        percent(&w["utilization"])
            .map(|p| json!({"usedPercent": p, "windowMins": mins, "resetsAt": w["resetsAt"]}))
    };
    let mut primary = window("five_hour", 300);
    let mut secondary = window("seven_day", 10080);
    if primary.is_none() && secondary.is_none() {
        if let Some(p) = percent(&info["utilization"]) {
            let w = json!({"usedPercent": p, "resetsAt": info["resetsAt"], "windowMins": if info["rateLimitType"] == "five_hour" { 300 } else { 10080 }});
            if info["rateLimitType"] == "five_hour" {
                primary = Some(w);
            } else if info["rateLimitType"].as_str().map(|t| t.starts_with("seven_day")).unwrap_or(false) {
                secondary = Some(w);
            }
        }
    }
    let mut usage = json!({"status": info["status"], "limitReached": info["status"] == "rejected"});
    if let Some(p) = primary {
        usage["primary"] = p;
    }
    if let Some(s) = secondary {
        usage["secondary"] = s;
    }
    if info["status"] == "rejected" {
        usage["limitResetsAt"] = info["resetsAt"].clone();
    }
    usage
}

pub fn normalize_models(models: &Value) -> Vec<Value> {
    models
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| {
                    let value = m["value"].as_str()?;
                    if value == "default" {
                        return None;
                    }
                    let efforts: Vec<Value> = m["supportedEffortLevels"]
                        .as_array()
                        .map(|l| {
                            l.iter()
                                .filter_map(|e| e.as_str())
                                .map(|e| json!({"reasoningEffort": e}))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(json!({"id": value, "model": value, "displayName": m["displayName"].as_str().unwrap_or(value), "description": m["description"], "supportedReasoningEfforts": efforts}))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn default_models() -> Vec<Value> {
    let efforts: Vec<Value> = crate::portable_paths::CLAUDE_EFFORTS
        .iter()
        .map(|e| json!({"reasoningEffort": e}))
        .collect();
    [("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")]
        .iter()
        .map(|(v, n)| json!({"id": v, "model": v, "displayName": n, "supportedReasoningEfforts": efforts}))
        .collect()
}

/// Read-only asks before every edit or command; workspace-write lets Claude edit project files.
pub fn permission_mode(c: &Config) -> &'static str {
    if c.sandbox == "read-only" {
        "default"
    } else {
        "acceptEdits"
    }
}

pub fn end_turn(r: &mut Runtime, interrupted: bool) {
    r.starting_turn = false;
    r.view.turn_id = None;
    r.claude_interrupting = false;
    r.pending.retain(|k, _| !k.starts_with("claude:"));
    r.view.approvals = r.pending.values().cloned().collect();
    r.view.lifecycle = "Ready".into();
    for item in r.view.items.iter_mut() {
        if item["status"] == "inProgress" {
            item["status"] = json!(if interrupted { "interrupted" } else { "completed" });
        }
    }
}

/// Applies one Claude frame. Returns a line to write back to Claude (permission answers).
pub fn handle(r: &mut Runtime, frame: &Value) -> Option<Value> {
    if let Some(e) = frame.get("localError").and_then(Value::as_str) {
        r.view.error = Some(e.into());
        return None;
    }
    if frame.get("localExit").is_some() {
        r.claude = None;
        let stderr = frame["stderr"].as_str().unwrap_or("").trim().to_string();
        let was_active = r.active();
        if stderr.contains("No conversation found") {
            r.claude_has_history = false;
        }
        if was_active {
            end_turn(r, true);
            r.view.error = Some(if stderr.is_empty() {
                "Claude stopped unexpectedly. Send your message again to continue.".into()
            } else {
                format!("Claude stopped unexpectedly: {stderr}")
            });
        } else if !stderr.is_empty() && frame["localExit"] != 0 {
            r.view.error = Some(format!("Claude could not start: {stderr}"));
        }
        return None;
    }
    match frame["type"].as_str().unwrap_or("") {
        "stream_event" => {
            if !frame["parent_tool_use_id"].is_null() {
                return None;
            }
            let ev = &frame["event"];
            match ev["type"].as_str() {
                Some("message_start") => {
                    r.claude_msg = ev["message"]["id"].as_str().map(str::to_string)
                }
                Some("content_block_start") => {
                    let msg = r.claude_msg.clone().unwrap_or_default();
                    let idx = ev["index"].as_u64().unwrap_or(0);
                    match ev["content_block"]["type"].as_str() {
                        Some("text") => upsert(
                            &mut r.view.items,
                            json!({"id": format!("{msg}:{idx}"), "type": "agentMessage", "text": ""}),
                        ),
                        Some("thinking") => upsert(
                            &mut r.view.items,
                            json!({"id": format!("{msg}:thinking"), "type": "reasoning", "status": "inProgress"}),
                        ),
                        _ => {}
                    }
                }
                Some("content_block_delta") if ev["delta"]["type"] == "text_delta" => {
                    let msg = r.claude_msg.clone().unwrap_or_default();
                    let id = format!("{msg}:{}", ev["index"].as_u64().unwrap_or(0));
                    if !r.view.items.iter().any(|i| i["id"] == id.as_str()) {
                        r.view.items.push(json!({"id": id, "type": "agentMessage", "text": ""}));
                    }
                    if let Some(item) = r.view.items.iter_mut().find(|i| i["id"] == id.as_str()) {
                        append(item, "text", ev["delta"]["text"].as_str().unwrap_or(""), 2 * 1024 * 1024);
                    }
                }
                _ => {}
            }
        }
        "assistant" => {
            if frame["error"].is_string() || frame["is_api_error_message"] == true {
                let text = frame["message"]["content"][0]["text"].as_str().unwrap_or("");
                r.view.error = Some(format!(
                    "{} ({})",
                    if text.is_empty() { "Claude could not answer" } else { text },
                    frame["error"].as_str().unwrap_or("error")
                ));
                return None;
            }
            if frame["parent_tool_use_id"].is_null() {
                apply_assistant(&mut r.view.items, &frame["message"]);
                r.trim_items();
            }
        }
        "user" => apply_tool_results(&mut r.view.items, &frame["message"]),
        "result" => {
            let interrupted = r.claude_interrupting;
            end_turn(r, interrupted);
            if frame["is_error"] == true && !interrupted && r.view.error.is_none() {
                let text = frame["result"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| {
                        frame["errors"].as_array().map(|e| {
                            e.iter()
                                .filter_map(|x| x.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                    })
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Claude stopped with an error.".into());
                r.view.error = Some(text);
            }
        }
        "rate_limit_event" => {
            let usage = usage_from_event(&frame["rate_limit_info"]);
            r.set_usage("claude", usage, true);
        }
        "control_request" => {
            let rid = frame["request_id"].as_str().unwrap_or("").to_string();
            let req = &frame["request"];
            if req["subtype"] != "can_use_tool" {
                return Some(json!({"type":"control_response","response":{"subtype":"error","request_id":rid,"error":"This request type is not supported by RustPortableHarness."}}));
            }
            if r.view.config.approval_policy == "never" {
                return Some(json!({"type":"control_response","response":{"subtype":"success","request_id":rid,"response":{"behavior":"allow","updatedInput":req["input"]}}}));
            }
            if r.pending.len() >= 32 {
                return Some(json!({"type":"control_response","response":{"subtype":"success","request_id":rid,"response":{"behavior":"deny","message":"Too many pending requests"}}}));
            }
            let key = format!("claude:{rid}");
            r.pending.insert(
                key.clone(),
                Approval {
                    key,
                    id: json!(rid),
                    method: APPROVAL_METHOD.into(),
                    params: json!({
                        "toolName": req["tool_name"],
                        "input": req["input"],
                        "decisionReason": req["decision_reason"].as_str().map(strip_ansi),
                        "blockedPath": req["blocked_path"],
                        "toolUseId": req["tool_use_id"],
                    }),
                    supported: true,
                },
            );
            r.view.approvals = r.pending.values().cloned().collect();
            r.view.lifecycle = "Waiting for Approval".into();
        }
        "control_cancel_request" => {
            let key = format!("claude:{}", frame["request_id"].as_str().unwrap_or(""));
            r.pending.remove(&key);
            r.view.approvals = r.pending.values().cloned().collect();
            if r.pending.is_empty() && r.view.lifecycle == "Waiting for Approval" {
                r.view.lifecycle = if r.view.turn_id.is_some() { "Busy" } else { "Ready" }.into();
            }
        }
        "control_response" => {
            let resp = &frame["response"];
            if resp["request_id"] == "init" && resp["subtype"] == "success" {
                let models = normalize_models(&resp["response"]["models"]);
                if !models.is_empty() && r.view.config.provider == "claude" {
                    r.view.models = models;
                }
            }
        }
        "system" => match frame["subtype"].as_str() {
            Some("init") => {
                r.view.claude_info = json!({
                    "model": frame["model"], "permissionMode": frame["permissionMode"],
                    "version": frame["claude_code_version"],
                    "shell": if frame["tools"].as_array().map(|t| t.iter().any(|x| x == "Bash")).unwrap_or(false) { "Bash" } else { "PowerShell" },
                });
            }
            Some(sub) if sub.contains("retry") => {
                r.view.warning = Some(format!(
                    "Claude is retrying after a connection problem{}.",
                    frame["attempt"].as_u64().map(|a| format!(" (attempt {a})")).unwrap_or_default()
                ));
            }
            _ => {}
        },
        _ => {}
    }
    None
}

pub fn decision_response(params: &Value, request_id: &Value, decision: &str) -> Result<Value, String> {
    let response = match decision {
        "accept" => json!({"behavior":"allow","updatedInput":params["input"]}),
        "decline" => json!({"behavior":"deny","message":"The user declined this action in RustPortableHarness. Continue without it or suggest another way."}),
        "cancel" => json!({"behavior":"deny","message":"The user stopped this turn.","interrupt":true}),
        _ => return Err("Unsupported approval decision".into()),
    };
    Ok(json!({"type":"control_response","response":{"subtype":"success","request_id":request_id,"response":response}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> Runtime {
        let mut c = Config::default();
        c.normalize();
        c.provider = "claude".into();
        let mut r = Runtime::new(c, "test".into());
        r.view.thread_id = Some("t".into());
        r.view.turn_id = Some("turn".into());
        r
    }
    #[test]
    fn streamed_text_is_finalised_without_duplicates() {
        let mut r = runtime();
        handle(&mut r, &json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"m1"}}}));
        handle(&mut r, &json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text"}}}));
        handle(&mut r, &json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}}));
        assert_eq!(r.view.items[0]["text"], "Hel");
        handle(&mut r, &json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"m1","content":[{"type":"text","text":"Hello there"}]}}));
        handle(&mut r, &json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_start","index":2,"content_block":{"type":"text"}}}));
        handle(&mut r, &json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Done"}}}));
        handle(&mut r, &json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"m1","content":[{"type":"text","text":"Done."}]}}));
        let texts: Vec<_> = r.view.items.iter().filter(|i| i["type"] == "agentMessage").map(|i| i["text"].as_str().unwrap().to_string()).collect();
        assert_eq!(texts, vec!["Hello there", "Done."]);
    }
    #[test]
    fn tools_map_to_activity_and_results_complete_them() {
        let mut r = runtime();
        handle(&mut r, &json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"m2","content":[
            {"type":"tool_use","id":"tu1","name":"Bash","input":{"command":"dir"}},
            {"type":"tool_use","id":"tu2","name":"Edit","input":{"file_path":"a.txt","old_string":"a","new_string":"b"}},
            {"type":"tool_use","id":"tu3","name":"Read","input":{"file_path":"a.txt"}}]}}));
        assert_eq!(r.view.items[0]["type"], "commandExecution");
        assert_eq!(r.view.items[1]["type"], "fileChange");
        assert!(r.view.items[1]["changes"][0]["diff"].as_str().unwrap().contains("+b"));
        assert_eq!(r.view.items[2]["type"], "toolCall");
        handle(&mut r, &json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"ok"},{"type":"tool_result","tool_use_id":"tu2","is_error":true,"content":"The user declined"}]}}));
        assert_eq!(r.view.items[0]["status"], "completed");
        assert_eq!(r.view.items[1]["status"], "declined");
        handle(&mut r, &json!({"type":"result","is_error":false,"result":"done"}));
        assert_eq!(r.view.items[2]["status"], "completed");
        assert!(r.view.turn_id.is_none());
        assert_eq!(r.view.lifecycle, "Ready");
    }
    #[test]
    fn permission_requests_wait_for_the_user_unless_auto_allow() {
        let mut r = runtime();
        let req = json!({"type":"control_request","request_id":"p1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"del x"},"tool_use_id":"tu"}});
        assert!(handle(&mut r, &req).is_none());
        assert_eq!(r.view.approvals.len(), 1);
        assert_eq!(r.view.lifecycle, "Waiting for Approval");
        let reply = decision_response(&r.view.approvals[0].params, &r.view.approvals[0].id, "decline").unwrap();
        assert_eq!(reply["response"]["response"]["behavior"], "deny");
        assert_eq!(reply["response"]["request_id"], "p1");
        handle(&mut r, &json!({"type":"control_cancel_request","request_id":"p1"}));
        assert!(r.view.approvals.is_empty());
        r.view.config.approval_policy = "never".into();
        let reply = handle(&mut r, &req).unwrap();
        assert_eq!(reply["response"]["response"]["behavior"], "allow");
        assert_eq!(reply["response"]["response"]["updatedInput"]["command"], "del x");
    }
    #[test]
    fn rate_limit_event_updates_active_account_usage() {
        let mut r = runtime();
        handle(&mut r, &json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.42,"resetsAt":100},"seven_day":{"utilization":0.1,"resetsAt":200}}}}));
        let a = r.view.config.active_account("claude");
        assert_eq!(a.usage.as_ref().unwrap()["primary"]["usedPercent"], 42);
        assert_eq!(a.usage.as_ref().unwrap()["secondary"]["usedPercent"], 10);
        handle(&mut r, &json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","unifiedWindows":{"five_hour":{"utilization":0.9,"resetsAt":100}}}}));
        let a = r.view.config.active_account("claude");
        assert_eq!(a.usage.as_ref().unwrap()["primary"]["usedPercent"], 90);
        assert_eq!(a.usage.as_ref().unwrap()["secondary"]["usedPercent"], 10, "absent windows keep their last value");
        assert!(r.config_dirty);
    }
    #[test]
    fn api_errors_become_notices_not_chat_messages() {
        let mut r = runtime();
        handle(&mut r, &json!({"type":"assistant","error":"authentication_failed","is_api_error_message":true,"message":{"id":"x","content":[{"type":"text","text":"Not logged in · Please run /login"}]}}));
        assert!(r.view.items.is_empty());
        assert!(r.view.error.as_deref().unwrap().contains("authentication_failed"));
    }
    #[test]
    fn unexpected_exit_ends_the_turn() {
        let mut r = runtime();
        handle(&mut r, &json!({"localExit":1,"stderr":"boom"}));
        assert!(r.view.turn_id.is_none());
        assert!(r.view.error.as_deref().unwrap().contains("boom"));
    }
    #[test]
    fn helpers() {
        assert_eq!(sanitize_cwd(r"C:\Users\Tom\my proj"), "C--Users-Tom-my-proj");
        assert_eq!(parse_version("2.1.308 (Claude Code)"), Some([2, 1, 308]));
        assert_eq!(
            find_url(&strip_ansi("If the browser didn't open, visit: \u{1b}]8;;https://claude.ai/x?a=1\u{7}https://claude.ai/x?a=1\u{1b}]8;;\u{7}\nPaste")),
            Some("https://claude.ai/x?a=1".into())
        );
        let args = Launch { binary: Path::new("c"), cwd: Path::new("."), env: &ChildEnv::default(), session_id: "11111111-2222-4333-8444-555555555555", resume: true, model: Some("sonnet"), effort: None, permission_mode: "default", project_settings: false, system_note: Some("tools") }.args();
        assert!(args.windows(2).any(|w| w[0] == "--resume"));
        assert!(args.windows(2).any(|w| w == ["--setting-sources", "user"]));
    }
    /// Drives a real claude.exe (signed out, isolated config) through spawn → initialize → turn → exit.
    /// Run with: CLAUDE_TEST_EXE=<path to claude.exe> cargo test real_claude -- --ignored
    #[tokio::test]
    #[ignore = "needs CLAUDE_TEST_EXE pointing at an official claude.exe"]
    async fn real_claude_session_signed_out() {
        let exe = PathBuf::from(std::env::var("CLAUDE_TEST_EXE").expect("CLAUDE_TEST_EXE"));
        let info = validate(exe.clone()).await.unwrap();
        assert!(info.version.contains("Claude Code"));
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let env = crate::netenv::claude_env(&Config::default(), home.path(), None, None);
        let status = auth_status(&exe, &env, cwd.path()).await.unwrap();
        assert_eq!(account_from_status(&status)["loggedIn"], false);
        let shared = Arc::new(Mutex::new(runtime()));
        let s = shared.clone();
        let handler: Handler = Arc::new(move |frame| {
            let mut r = s.lock().unwrap();
            handle(&mut r, &frame);
        });
        let id = uuid::Uuid::new_v4().to_string();
        let launch = Launch { binary: &exe, cwd: cwd.path(), env: &env, session_id: &id, resume: false, model: None, effort: None, permission_mode: "default", project_settings: false, system_note: None };
        let session = Session::spawn(launch, "sig".into(), handler).await.unwrap();
        shared.lock().unwrap().claude = Some(session.clone());
        session.user_message("hi").await.unwrap();
        for _ in 0..600 {
            if shared.lock().unwrap().view.turn_id.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        {
            let r = shared.lock().unwrap();
            assert!(r.view.turn_id.is_none(), "turn should finish");
            assert!(r.view.models.iter().any(|m| m["id"] == "opus"), "models from initialize: {:?}", r.view.models);
            assert!(r.view.error.as_deref().unwrap_or("").contains("authentication"), "error: {:?}", r.view.error);
        }
        session.stop().await;
        assert!(!session.alive());
    }
    #[test]
    fn history_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let cwd = r"C:\proj";
        let dir = t.path().join("projects").join(sanitize_cwd(cwd));
        std::fs::create_dir_all(&dir).unwrap();
        let id = "11111111-2222-4333-8444-555555555555";
        std::fs::write(dir.join(format!("{id}.jsonl")), [
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"fix the bug"}]},"uuid":"u1"}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Looking"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"dir"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a.txt"}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"<local-command-stdout>x</local-command-stdout>"}}"#,
        ].join("\n")).unwrap();
        let threads = list_threads(t.path(), cwd);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0]["name"], "fix the bug");
        let items = load_history(t.path(), cwd, id).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[2]["status"], "completed");
        archive_thread(t.path(), cwd, id).unwrap();
        assert!(list_threads(t.path(), cwd).is_empty());
    }
}
