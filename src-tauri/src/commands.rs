use crate::{
    claude, netenv, scaffold,
    state::Job,
    tools::{self, Layout},
    portable_paths::{
        broad_folder_warning, canonical_project, valid_account_id, Account, Config,
        PortablePaths, CLAUDE_DEFAULT, CLAUDE_EFFORTS, CODEX_DEFAULT,
    },
    protocol,
    rpc_transport::{EventHandler, Rpc},
    state::{codex_usage, BinaryInfo, Runtime, Shared, Snapshot},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, State};
pub struct AppState {
    pub paths: PortablePaths,
    pub shared: Shared,
    pub operation: tokio::sync::Mutex<()>,
    /// Held for the app's lifetime so a second copy cannot share the data folder.
    pub _instance: Option<std::fs::File>,
}
const NO_CLAUDE: &str = "Claude is not included in this copy of the app. Ask your teacher for the version with claude.exe, or choose claude.exe in Settings → Advanced.";
fn idle(r: &Runtime) -> Result<(), String> {
    if r.active() {
        Err(
            "Stop the current turn before changing projects, threads, accounts, or connection settings."
                .into(),
        )
    } else {
        Ok(())
    }
}
fn save(s: &AppState) -> Result<(), String> {
    let config = s.shared.lock().unwrap().view.config.clone();
    s.paths.save(&config)
}
fn save_if_dirty(s: &AppState) {
    let dirty = {
        let mut r = s.shared.lock().unwrap();
        std::mem::take(&mut r.config_dirty).then(|| r.view.config.clone())
    };
    if let Some(c) = dirty {
        let _ = s.paths.save(&c);
    }
}
fn changed(s: &AppState, app: &AppHandle) {
    s.shared.lock().unwrap().changed(app);
}
fn rpc(s: &AppState) -> Result<Rpc, String> {
    s.shared.lock().unwrap().rpc()
}
fn config(s: &AppState) -> Config {
    s.shared.lock().unwrap().view.config.clone()
}
fn provider(s: &AppState) -> String {
    s.shared.lock().unwrap().view.config.provider.clone()
}
fn binary(s: &AppState) -> PathBuf {
    s.shared
        .lock()
        .unwrap()
        .view
        .config
        .binary_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| s.paths.root.join("codex.exe"))
}
fn project(s: &AppState) -> Result<String, String> {
    let p = s
        .shared
        .lock()
        .unwrap()
        .view
        .config
        .selected_project
        .clone()
        .ok_or("Choose a project folder first.")?;
    canonical_project(&p)
}
fn active_home(s: &AppState, provider: &str) -> Result<PathBuf, String> {
    let account = config(s).active_account(provider);
    s.paths.account_home(&account)
}
fn claude_binary(s: &AppState) -> Result<PathBuf, String> {
    claude::find_binary(&s.paths.root, &config(s)).ok_or_else(|| NO_CLAUDE.to_string())
}
fn git_bash(s: &AppState) -> Option<PathBuf> {
    netenv::bundled_git_bash(&s.paths.root)
}
fn claude_env_for(s: &AppState, account: &Account) -> Result<netenv::ChildEnv, String> {
    let home = s.paths.account_home(account)?;
    let mut env = netenv::claude_env(
        &config(s),
        &home,
        git_bash(s).as_deref(),
        Some(&s.paths.data.join("temp")),
    );
    env.extend(&tools::env(&layout(&s.paths)));
    Ok(env)
}
fn claude_env(s: &AppState) -> Result<netenv::ChildEnv, String> {
    claude_env_for(s, &config(s).active_account("claude"))
}
/// The selected project, or the portable temp folder when none is chosen yet.
fn work_dir(s: &AppState) -> PathBuf {
    project(s)
        .map(PathBuf::from)
        .unwrap_or_else(|_| s.paths.data.join("temp"))
}
pub(crate) fn layout(paths: &PortablePaths) -> Layout {
    Layout::new(&paths.root, &paths.data)
}
/// Proxy, certificate, temp folder and every portable tool: what Run, Terminal and setup jobs use.
fn tool_env(s: &AppState) -> netenv::ChildEnv {
    let mut env = netenv::network_env(&config(s), Some(&s.paths.data.join("temp")));
    env.extend(&tools::env(&layout(&s.paths)));
    env
}
/// Tool states, the selected project's Run command and whether Codex needs a restart to see new tools.
pub(crate) fn refresh_env_info(paths: &PortablePaths, r: &mut Runtime) {
    let l = layout(paths);
    r.view.tools = tools::statuses(&l);
    r.view.templates = scaffold::templates_json();
    r.view.projects_dir = crate::portable_paths::display_path(&l.projects());
    r.view.project_info = tools::project_info(&l, r.view.config.selected_project.as_deref());
    r.view.tools_stale = r.rpc.is_some() && r.view.config.provider == "codex" && r.agent_tools != tools::fingerprint(&l);
}
pub(crate) fn engines(paths: &PortablePaths, c: &Config) -> Value {
    let codex = c
        .binary_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.root.join("codex.exe"));
    let claude = claude::find_binary(&paths.root, c);
    let bundled = netenv::bundled_git_bash(&paths.root);
    let system = netenv::system_git_bash();
    json!({
        "codexFound": codex.is_file(),
        "claudeFound": claude.is_some(),
        "claudePath": claude.map(|p| crate::portable_paths::display_path(&p)),
        "gitBash": bundled.as_ref().or(system.as_ref()).map(|p| crate::portable_paths::display_path(p)),
        "gitBashBundled": bundled.is_some(),
    })
}

/// Checks that a file is a Windows x64 program and returns its SHA-256.
pub(crate) fn hash_executable(path: &Path, name: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| {
        format!("{name} is missing or cannot be opened: {e}. Extract the ZIP again; if it keeps disappearing, antivirus may be removing it.")
    })?;
    let mut dos = [0u8; 64];
    f.read_exact(&mut dos)
        .map_err(|_| format!("{name} is not a valid program"))?;
    if &dos[..2] != b"MZ" {
        return Err(format!("{name} must be a Windows x64 program"));
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    f.seek(SeekFrom::Start(offset as u64))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 6];
    f.read_exact(&mut pe).map_err(|e| e.to_string())?;
    if &pe[..4] != b"PE\0\0" || pe[4..] != [0x64, 0x86] {
        return Err(format!("{name} must be a Windows x64 program"));
    }
    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
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
pub async fn validate_binary(path: PathBuf) -> Result<BinaryInfo, String> {
    let p = path.clone();
    let hash = tokio::task::spawn_blocking(move || hash_executable(&p, "codex.exe"))
        .await
        .map_err(|e| e.to_string())??;
    let mut c = tokio::process::Command::new(&path);
    c.arg("--version").kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    // Antivirus scans a large new executable on its first run; allow a generous first start.
    let out = tokio::time::timeout(Duration::from_secs(90), c.output())
        .await
        .map_err(|_| "codex.exe did not answer. Windows may still be scanning it (the first start can take a minute). Try again shortly.")?
        .map_err(|e| format!("Cannot run codex.exe: {e}. Windows or school policy may be blocking it."))?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || version != protocol::CODEX_VERSION {
        return Err(format!("Incompatible Codex binary: {version}. This release requires {} and its generated protocol. Regenerate bindings before upgrading.",protocol::CODEX_VERSION));
    }
    Ok(BinaryInfo {
        path: path.to_string_lossy().into(),
        version,
        sha256: hash,
    })
}

/// Stops whichever agent is running. Bumped generations make late events from it harmless.
async fn stop_engines(s: &AppState) {
    let (rpc, session) = {
        let mut r = s.shared.lock().unwrap();
        r.view.generation += 1;
        r.claude_gen += 1;
        r.pending.clear();
        r.view.approvals.clear();
        r.view.turn_id = None;
        r.starting_turn = false;
        r.claude_interrupting = false;
        (r.rpc.take(), r.claude.take())
    };
    if let Some(rpc) = rpc {
        rpc.stop().await;
    }
    if let Some(session) = session {
        session.stop().await;
    }
}
fn reset_view(r: &mut Runtime, lifecycle: &str) {
    r.view.lifecycle = lifecycle.into();
    r.view.models.clear();
    r.view.threads.clear();
    r.view.items.clear();
    r.view.diff.clear();
    r.view.thread_id = None;
    r.view.thread_cursor = None;
    r.view.history_cursor = None;
    r.view.account = Value::Null;
    r.view.login = Value::Null;
    r.view.claude_info = Value::Null;
    r.view.error = None;
}

#[tauri::command]
pub async fn snapshot(s: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(s.shared.lock().unwrap().view.clone())
}
#[tauri::command]
pub async fn connect(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    stop_engines(&s).await;
    {
        let mut r = s.shared.lock().unwrap();
        reset_view(&mut r, "Starting");
        let engines = engines(&s.paths, &r.view.config);
        r.view.engines = engines;
    }
    changed(&s, &app);
    let result = if provider(&s) == "claude" {
        connect_claude(&app, &s).await
    } else {
        connect_inner(&app, &s).await
    };
    if let Err(e) = &result {
        s.shared.lock().unwrap().fail(e.clone());
    }
    refresh_env_info(&s.paths, &mut s.shared.lock().unwrap());
    changed(&s, &app);
    save_if_dirty(&s);
    result
}
fn apply_codex_account(r: &mut Runtime, account: Value) {
    let email = account["account"]["email"].as_str().map(str::to_string);
    let plan = account["account"]["planType"].as_str().map(str::to_string);
    let id = r.view.config.active_account("codex").id;
    if let Some(a) = r.view.config.account_mut(&id) {
        if a.email != email || a.plan != plan {
            a.email = email;
            a.plan = plan;
            r.config_dirty = true;
        }
    }
    r.view.account = account;
}
async fn refresh_codex_account(
    shared: &Shared,
    paths: &PortablePaths,
    app: &AppHandle,
    rpc: &Rpc,
    generation: u64,
) {
    let account = rpc
        .request("account/read", json!({"refreshToken":false}))
        .await;
    let limits = rpc
        .request(
            "account/rateLimits/read",
            json!({"excludeResetCreditDetails":true}),
        )
        .await;
    let dirty = {
        let mut guard = shared.lock().unwrap();
        let r: &mut Runtime = &mut guard;
        if r.view.generation != generation {
            return;
        }
        if let Ok(a) = account {
            apply_codex_account(r, a);
        }
        if let Ok(l) = limits {
            r.set_usage("codex", codex_usage(&l["rateLimits"]), false);
        }
        r.changed(app);
        std::mem::take(&mut r.config_dirty).then(|| r.view.config.clone())
    };
    if let Some(c) = dirty {
        let _ = paths.save(&c);
    }
}
async fn connect_inner(app: &AppHandle, s: &AppState) -> Result<(), String> {
    let info = validate_binary(binary(s)).await?;
    let cwd = if s
        .shared
        .lock()
        .unwrap()
        .view
        .config
        .selected_project
        .is_some()
    {
        PathBuf::from(project(s)?)
    } else {
        s.paths.data.join("temp")
    };
    let home = active_home(s, "codex")?;
    let mut env = netenv::codex_env(&config(s), Some(&s.paths.data.join("temp")));
    let l = layout(&s.paths);
    env.extend(&tools::env(&l));
    // Codex reads $CODEX_HOME/AGENTS.md: tell it which portable tools exist here.
    let _ = tools::write_notes_block(&home.join("AGENTS.md"), &tools::agent_notes(&l));
    let fingerprint = tools::fingerprint(&l);
    let generation = {
        let mut r = s.shared.lock().unwrap();
        r.view.binary = Some(info.clone());
        r.view.lifecycle = "Handshaking".into();
        r.view.generation
    };
    changed(s, app);
    let shared = s.shared.clone();
    let a = app.clone();
    let paths = s.paths.clone();
    let cell: Arc<Mutex<Option<Rpc>>> = Default::default();
    let out = cell.clone();
    let log_path = s.paths.data.join("logs/backend.log");
    let handler: EventHandler = Arc::new(move |frame| {
        let method = frame["method"].as_str().unwrap_or("").to_string();
        let mut guard = shared.lock().unwrap();
        let r: &mut Runtime = &mut guard;
        if r.view.generation != generation {
            return;
        }
        if r.view.config.debug_log {
            use std::io::Write;
            let method = frame["method"].as_str().unwrap_or("lifecycle");
            let safe: String = method
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || ['/', '_'].contains(c))
                .take(120)
                .collect();
            if std::fs::metadata(&log_path)
                .map(|m| m.len() > 262144)
                .unwrap_or(false)
            {
                let _ = std::fs::write(&log_path, b"");
            }
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = writeln!(f, "generation={generation} method={safe}");
            }
        }
        let reply = r.handle(frame);
        let dirty = std::mem::take(&mut r.config_dirty).then(|| r.view.config.clone());
        r.changed(&a);
        drop(guard);
        if let Some(c) = dirty {
            let _ = paths.save(&c);
        }
        let rpc = out.lock().unwrap().clone();
        if let (Some(reply), Some(rpc)) = (reply, rpc.clone()) {
            tauri::async_runtime::spawn(async move {
                let _ = rpc.write(reply).await;
            });
        }
        // Usage and the signed-in email change after turns and sign-ins.
        if matches!(method.as_str(), "turn/completed" | "account/login/completed") {
            if let Some(rpc) = rpc {
                let (shared, paths, app) = (shared.clone(), paths.clone(), a.clone());
                tauri::async_runtime::spawn(async move {
                    refresh_codex_account(&shared, &paths, &app, &rpc, generation).await;
                });
            }
        }
    });
    let rpc = Rpc::spawn(Path::new(&info.path), &home, &cwd, &env, handler).await?;
    *cell.lock().unwrap() = Some(rpc.clone());
    if let Err(e) = rpc.handshake().await {
        rpc.stop().await;
        return Err(format!("App Server handshake failed: {e}"));
    }
    {
        let mut r = s.shared.lock().unwrap();
        r.rpc = Some(rpc.clone());
        r.agent_tools = fingerprint;
        r.view.tools_stale = false;
        r.view.lifecycle = "Ready".into();
        r.view.thread_id = None;
        r.view.turn_id = None;
        r.view.items.clear();
        r.view.diff.clear();
    }
    let models = rpc
        .request("model/list", json!({"limit":100,"includeHidden":false}))
        .await;
    {
        let mut r = s.shared.lock().unwrap();
        match models {
            Ok(m) => r.view.models = m["data"].as_array().cloned().unwrap_or_default(),
            Err(e) => r.view.warning = Some(e),
        }
    }
    refresh_codex_account(&s.shared, &s.paths, app, &rpc, generation).await;
    if s.shared
        .lock()
        .unwrap()
        .view
        .config
        .selected_project
        .is_some()
    {
        let _ = refresh_threads_inner(s, false).await;
    }
    changed(s, app);
    Ok(())
}
fn apply_claude_status(r: &mut Runtime, status: &Value) {
    let account = claude::account_from_status(status);
    let email = account["account"]["email"].as_str().map(str::to_string);
    let plan = account["account"]["planType"].as_str().map(str::to_string);
    let id = r.view.config.active_account("claude").id;
    if let Some(a) = r.view.config.account_mut(&id) {
        if a.email != email || a.plan != plan {
            a.email = email;
            a.plan = plan;
            r.config_dirty = true;
        }
    }
    r.view.account = account;
}
async fn connect_claude(app: &AppHandle, s: &AppState) -> Result<(), String> {
    let binary = claude_binary(s)?;
    let info = claude::validate(binary.clone()).await?;
    {
        let mut r = s.shared.lock().unwrap();
        r.view.claude_binary = Some(info);
        r.view.lifecycle = "Handshaking".into();
    }
    changed(s, app);
    let env = claude_env(s)?;
    let status = claude::auth_status(&binary, &env, &work_dir(s)).await;
    {
        let mut guard = s.shared.lock().unwrap();
        let r: &mut Runtime = &mut guard;
        match status {
            Ok(v) => apply_claude_status(r, &v),
            Err(e) => r.view.warning = Some(e),
        }
        r.view.lifecycle = "Ready".into();
        r.view.models = claude::default_models();
        r.view.thread_id = None;
        r.view.turn_id = None;
        r.view.items.clear();
    }
    if s.shared
        .lock()
        .unwrap()
        .view
        .config
        .selected_project
        .is_some()
    {
        let _ = refresh_threads_inner(s, false).await;
    }
    save_if_dirty(s);
    changed(s, app);
    Ok(())
}
#[tauri::command]
pub async fn disconnect(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let _op = s.operation.lock().await;
    stop_engines(&s).await;
    {
        let mut r = s.shared.lock().unwrap();
        r.view.lifecycle = "Stopped".into();
    }
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn set_provider(
    app: AppHandle,
    s: State<'_, AppState>,
    provider: String,
) -> Result<(), String> {
    if !["codex", "claude"].contains(&provider.as_str()) {
        return Err("Unknown AI provider".into());
    }
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    if self::provider(&s) == provider {
        return Ok(());
    }
    stop_engines(&s).await;
    {
        let mut r = s.shared.lock().unwrap();
        reset_view(&mut r, "Stopped");
        r.view.config.provider = provider;
        r.view.config.selected_thread = None;
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
async fn set_project(s: &AppState, path: String) -> Result<(), String> {
    let canonical = canonical_project(&path)?;
    let (was_connected, session) = {
        let mut r = s.shared.lock().unwrap();
        idle(&r)?;
        if !r.view.config.projects.contains(&canonical) {
            r.view.config.projects.push(canonical.clone());
        }
        r.view.config.selected_project = Some(canonical);
        r.view.config.selected_thread = None;
        r.view.thread_id = None;
        r.view.items.clear();
        r.view.diff.clear();
        r.view.threads.clear();
        r.view.thread_cursor = None;
        r.view.history_cursor = None;
        let connected = r.rpc.is_some() || r.view.claude_binary.is_some() && r.view.config.provider == "claude";
        let live = ["Ready", "Busy", "Waiting for Approval"].contains(&r.view.lifecycle.as_str());
        r.view.lifecycle = if connected && live { "Ready" } else { "Stopped" }.into();
        r.claude_gen += 1;
        (connected && live, r.claude.take())
    };
    if let Some(session) = session {
        session.stop().await;
    }
    save(s)?;
    refresh_env_info(&s.paths, &mut s.shared.lock().unwrap());
    if was_connected {
        let _ = refresh_threads_inner(s, false).await;
    }
    Ok(())
}
#[tauri::command]
pub async fn select_project(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    if let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("Choose the project folder the AI may work in")
        .pick_folder()
        .await
    {
        let path = canonical_project(&folder.path().to_string_lossy())?;
        if let Some(warning) = broad_folder_warning(&path) {
            let answer = rfd::AsyncMessageDialog::new()
                .set_title("Use this whole folder?")
                .set_description(format!("{warning}\n\nUse \"{path}\" anyway?"))
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await;
            if answer != rfd::MessageDialogResult::Yes {
                return Ok(());
            }
        }
        let _op = s.operation.lock().await;
        set_project(&s, path).await?;
        changed(&s, &app);
    }
    Ok(())
}
#[tauri::command]
pub async fn reconnect_project(
    app: AppHandle,
    s: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    if !s
        .shared
        .lock()
        .unwrap()
        .view
        .config
        .projects
        .contains(&path)
    {
        return Err("Choose this folder using Add project first".into());
    }
    set_project(&s, path).await?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn forget_project(
    app: AppHandle,
    s: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        let mut r = s.shared.lock().unwrap();
        idle(&r)?;
        r.view.config.projects.retain(|p| p != &path);
        if r.view.config.selected_project.as_deref() == Some(&path) {
            r.view.config.selected_project = None;
            r.view.config.selected_thread = None;
            r.view.thread_id = None;
            r.view.items.clear();
            r.view.threads.clear();
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
async fn refresh_threads_inner(s: &AppState, more: bool) -> Result<(), String> {
    let p = project(s)?;
    if provider(s) == "claude" {
        let home = active_home(s, "claude")?;
        let threads = tokio::task::spawn_blocking(move || claude::list_threads(&home, &p))
            .await
            .map_err(|e| e.to_string())?;
        let mut r = s.shared.lock().unwrap();
        r.view.threads = threads;
        r.view.thread_cursor = None;
        return Ok(());
    }
    let cursor = if more {
        s.shared.lock().unwrap().view.thread_cursor.clone()
    } else {
        None
    };
    let result = rpc(s)?
        .request(
            "thread/list",
            json!({"cwd":p,"limit":50,"sortKey":"updated_at","cursor":cursor,"sourceKinds":[]}),
        )
        .await?;
    let mut r = s.shared.lock().unwrap();
    let data = result["data"].as_array().cloned().unwrap_or_default();
    if more {
        r.view.threads.extend(data);
    } else {
        r.view.threads = data;
    }
    r.view.thread_cursor = result["nextCursor"].as_str().map(str::to_string);
    Ok(())
}
#[tauri::command]
pub async fn refresh_threads(
    app: AppHandle,
    s: State<'_, AppState>,
    more: bool,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    refresh_threads_inner(&s, more).await?;
    changed(&s, &app);
    Ok(())
}
fn policies(s: &AppState) -> Result<Value, String> {
    let r = s.shared.lock().unwrap();
    let c = &r.view.config;
    if !["read-only", "workspace-write", "danger-full-access"].contains(&c.sandbox.as_str())
        || !["on-request", "never"].contains(&c.approval_policy.as_str())
        || !["auto", "concise", "detailed", "none"].contains(&c.reasoning_summary.as_str())
    {
        return Err("Unsupported saved policy. Select a supported mode in Settings.".into());
    }
    let mut result = json!({"cwd":project_from_config(c)?,"sandbox":c.sandbox,"approvalPolicy":c.approval_policy,"approvalsReviewer":"user"});
    if let Some(tier) = &c.service_tier {
        result["serviceTier"] = json!(tier);
    }
    Ok(result)
}
fn project_from_config(c: &Config) -> Result<String, String> {
    canonical_project(
        c.selected_project
            .as_deref()
            .ok_or("Choose a project first")?,
    )
}
#[tauri::command]
pub async fn open_thread(
    app: AppHandle,
    s: State<'_, AppState>,
    id: Option<String>,
    more: bool,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    if provider(&s) == "claude" {
        return open_claude_thread(&app, &s, id, more).await;
    }
    let rpc = rpc(&s)?;
    if more {
        let thread = s
            .shared
            .lock()
            .unwrap()
            .view
            .thread_id
            .clone()
            .ok_or("Open a thread first")?;
        let cursor = s.shared.lock().unwrap().view.history_cursor.clone();
        let history=rpc.request("thread/turns/list",json!({"threadId":thread,"cursor":cursor,"limit":25,"sortDirection":"desc","itemsView":"full"})).await?;
        merge_history(&s, history, true);
        changed(&s, &app);
        return Ok(());
    }
    let mut params = policies(&s)?;
    if let Some(model) = s.shared.lock().unwrap().view.config.model.clone() {
        params["model"] = json!(model);
    }
    let method = if let Some(id) = id.as_ref() {
        let thread = rpc.request("thread/read", json!({"threadId":id})).await?;
        let cwd = thread["thread"]["cwd"]
            .as_str()
            .ok_or("Thread has no workspace")?;
        if canonical_project(cwd)? != project(&s)? {
            return Err("This thread belongs to a different project. Select its project before reopening it.".into());
        }
        params["threadId"] = json!(id);
        params["excludeTurns"] = json!(true);
        "thread/resume"
    } else {
        "thread/start"
    };
    let result = rpc.request(method, params).await?;
    let id = result["thread"]["id"]
        .as_str()
        .ok_or("Codex returned no thread ID")?
        .to_string();
    {
        let mut r = s.shared.lock().unwrap();
        r.view.thread_id = Some(id.clone());
        r.view.config.selected_thread = Some(id.clone());
        r.view.items.clear();
        r.view.diff.clear();
        r.view.history_cursor = None;
    }
    save(&s)?;
    if method == "thread/resume" {
        let history = rpc
            .request(
                "thread/turns/list",
                json!({"threadId":id,"limit":25,"sortDirection":"desc","itemsView":"full"}),
            )
            .await?;
        merge_history(&s, history, false);
    }
    let _ = refresh_threads_inner(&s, false).await;
    changed(&s, &app);
    Ok(())
}
async fn open_claude_thread(
    app: &AppHandle,
    s: &AppState,
    id: Option<String>,
    more: bool,
) -> Result<(), String> {
    if more {
        return Ok(());
    }
    let p = project(s)?;
    let home = active_home(s, "claude")?;
    claude_binary(s)?;
    let (id, existing) = match id {
        Some(id) if claude::valid_session_id(&id) => (id, true),
        Some(_) => return Err("Unknown conversation".into()),
        None => (uuid::Uuid::new_v4().to_string(), false),
    };
    let items = if existing {
        let (h, p2, i2) = (home.clone(), p.clone(), id.clone());
        tokio::task::spawn_blocking(move || claude::load_history(&h, &p2, &i2))
            .await
            .map_err(|e| e.to_string())??
    } else {
        vec![]
    };
    let old = {
        let mut r = s.shared.lock().unwrap();
        r.claude_gen += 1;
        r.view.thread_id = Some(id.clone());
        r.view.config.selected_thread = Some(id);
        r.view.items = items;
        r.view.diff.clear();
        r.view.history_cursor = None;
        r.view.error = None;
        r.claude_has_history = existing;
        r.claude_msg = None;
        r.claude.take()
    };
    if let Some(old) = old {
        old.stop().await;
    }
    save(s)?;
    changed(s, app);
    // Start Claude now so the first reply is quicker and the model list is current.
    if let Err(e) = ensure_claude_session(app, s).await {
        s.shared.lock().unwrap().view.error = Some(e);
    }
    let _ = refresh_threads_inner(s, false).await;
    changed(s, app);
    Ok(())
}
fn claude_handler(
    s: &AppState,
    app: &AppHandle,
    generation: u64,
    home: PathBuf,
    project: String,
) -> claude::Handler {
    let shared = s.shared.clone();
    let a = app.clone();
    let paths = s.paths.clone();
    Arc::new(move |frame| {
        // A finished reply means the chat file exists or got a new title: refresh the chat list.
        if frame["type"] == "result" {
            let (shared, a, home, project) = (shared.clone(), a.clone(), home.clone(), project.clone());
            tauri::async_runtime::spawn(async move {
                let Ok(threads) =
                    tokio::task::spawn_blocking(move || claude::list_threads(&home, &project)).await
                else {
                    return;
                };
                let mut r = shared.lock().unwrap();
                if r.claude_gen == generation && r.view.config.provider == "claude" {
                    r.view.threads = threads;
                    r.changed(&a);
                }
            });
        }
        let (reply, session, dirty) = {
            let mut guard = shared.lock().unwrap();
            let r: &mut Runtime = &mut guard;
            if r.claude_gen != generation {
                return;
            }
            let reply = claude::handle(r, &frame);
            let dirty = std::mem::take(&mut r.config_dirty).then(|| r.view.config.clone());
            r.changed(&a);
            (reply, r.claude.clone(), dirty)
        };
        if let Some(c) = dirty {
            let _ = paths.save(&c);
        }
        if let (Some(reply), Some(session)) = (reply, session) {
            tauri::async_runtime::spawn(async move {
                let _ = session.send(reply).await;
            });
        }
    })
}
/// Returns the running Claude process for the open thread, restarting it with `--resume` when the
/// model, effort, permission mode, account or network settings changed since it started.
async fn ensure_claude_session(app: &AppHandle, s: &AppState) -> Result<claude::Session, String> {
    let c = config(s);
    let thread = s
        .shared
        .lock()
        .unwrap()
        .view
        .thread_id
        .clone()
        .ok_or("Create or reopen a thread first")?;
    let p = project(s)?;
    let account = c.active_account("claude");
    let mode = claude::permission_mode(&c);
    let l = layout(&s.paths);
    let notes = tools::agent_notes(&l);
    let signature = format!(
        "{}|{}|{:?}|{:?}|{}|{}|{}|{:?}|{}|{}",
        tools::fingerprint(&l),
        account.id,
        c.claude_model,
        c.claude_effort,
        mode,
        c.claude_project_settings,
        c.proxy,
        c.ca_certificate,
        p,
        thread
    );
    let existing = s.shared.lock().unwrap().claude.clone();
    if let Some(session) = existing {
        if session.alive() && session.signature == signature && session.id == thread {
            return Ok(session);
        }
    }
    let (old, generation) = {
        let mut r = s.shared.lock().unwrap();
        r.claude_gen += 1;
        (r.claude.take(), r.claude_gen)
    };
    if let Some(old) = old {
        old.stop().await;
    }
    let binary = claude_binary(s)?;
    let home = s.paths.account_home(&account)?;
    let env = claude_env(s)?;
    let cwd = PathBuf::from(&p);
    let resume = claude::session_file(&home, &p, &thread).is_some();
    let launch = claude::Launch {
        binary: &binary,
        cwd: &cwd,
        env: &env,
        session_id: &thread,
        resume,
        model: c.claude_model.as_deref(),
        effort: c.claude_effort.as_deref(),
        permission_mode: mode,
        project_settings: c.claude_project_settings,
        system_note: Some(&notes),
    };
    let session =
        claude::Session::spawn(
        launch,
        signature,
        claude_handler(s, app, generation, home.clone(), p.clone()),
    )
    .await?;
    let mut r = s.shared.lock().unwrap();
    if r.claude_gen != generation {
        session.stop_now();
        return Err("Claude was restarted; try again.".into());
    }
    r.claude = Some(session.clone());
    Ok(session)
}
fn merge_history(s: &AppState, history: Value, prepend: bool) {
    let mut turns = history["data"].as_array().cloned().unwrap_or_default();
    turns.reverse();
    let items: Vec<Value> = turns
        .into_iter()
        .flat_map(|t| t["items"].as_array().cloned().unwrap_or_default())
        .collect();
    let mut r = s.shared.lock().unwrap();
    let mut combined = items;
    if prepend {
        combined.append(&mut r.view.items);
    }
    r.view.items = combined;
    r.view.history_cursor = history["nextCursor"].as_str().map(str::to_string);
}
#[tauri::command]
pub async fn rename_thread(
    app: AppHandle,
    s: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<(), String> {
    if label.trim().is_empty() || label.len() > 200 {
        return Err("Use a label of 1–200 characters".into());
    }
    let _op = s.operation.lock().await;
    {
        let mut r = s.shared.lock().unwrap();
        if !r.view.threads.iter().any(|t| t["id"] == id) && r.view.thread_id.as_deref() != Some(&id)
        {
            return Err("Unknown thread".into());
        }
        r.view.config.labels.insert(id, label);
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn archive_thread(
    app: AppHandle,
    s: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        let r = s.shared.lock().unwrap();
        idle(&r)?;
        if !r.view.threads.iter().any(|t| t["id"] == id) {
            return Err("Unknown thread".into());
        }
    }
    if provider(&s) == "claude" {
        let home = active_home(&s, "claude")?;
        claude::archive_thread(&home, &project(&s)?, &id)?;
        let old = {
            let mut r = s.shared.lock().unwrap();
            if r.view.thread_id.as_deref() == Some(&id) {
                r.claude_gen += 1;
                r.view.thread_id = None;
                r.view.config.selected_thread = None;
                r.view.items.clear();
                r.claude.take()
            } else {
                None
            }
        };
        if let Some(old) = old {
            old.stop().await;
        }
    } else {
        rpc(&s)?
            .request("thread/archive", json!({"threadId":id}))
            .await?;
        let mut r = s.shared.lock().unwrap();
        if r.view.thread_id.as_deref() == Some(&id) {
            r.view.thread_id = None;
            r.view.config.selected_thread = None;
            r.view.items.clear();
        }
    }
    save(&s)?;
    refresh_threads_inner(&s, false).await?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn send_turn(app: AppHandle, s: State<'_, AppState>, text: String) -> Result<(), String> {
    let _op = s.operation.lock().await;
    if text.trim().is_empty() || text.len() > 128 * 1024 {
        return Err("Enter a message shorter than 128 KiB".into());
    }
    if provider(&s) == "claude" {
        return send_claude_turn(&app, &s, text).await;
    }
    let (rpc, id, params) = {
        let mut r = s.shared.lock().unwrap();
        idle(&r)?;
        let rpc = r.rpc()?;
        let id = r
            .view
            .thread_id
            .clone()
            .ok_or("Create or reopen a thread first")?;
        let mut params = json!({"threadId":id,"input":[{"type":"text","text":text}],"model":r.view.config.model,"cwd":project_from_config(&r.view.config)?,"approvalPolicy":r.view.config.approval_policy,"approvalsReviewer":"user","summary":r.view.config.reasoning_summary});
        if let Some(effort) = &r.view.config.reasoning_effort {
            params["effort"] = json!(effort);
        }
        if let Some(tier) = &r.view.config.service_tier {
            params["serviceTier"] = json!(tier);
            params["serviceTierForTurn"] = json!(tier);
        }
        r.starting_turn = true;
        r.view.lifecycle = "Busy".into();
        r.view.error = None;
        (rpc, id, params)
    };
    changed(&s, &app);
    let result = rpc.request("turn/start", params).await;
    match result {
        Ok(turn) => {
            let mut r = s.shared.lock().unwrap();
            if r.starting_turn && r.view.thread_id.as_ref() == Some(&id) {
                r.view.turn_id = turn["turn"]["id"].as_str().map(str::to_string);
                r.starting_turn = false;
            }
        }
        Err(e) => {
            let mut r = s.shared.lock().unwrap();
            r.starting_turn = false;
            if r.view.turn_id.is_none() {
                r.view.lifecycle = "Ready".into();
            }
            r.view.error = Some(e.clone());
            r.changed(&app);
            return Err(e);
        }
    }
    changed(&s, &app);
    Ok(())
}
async fn send_claude_turn(app: &AppHandle, s: &AppState, text: String) -> Result<(), String> {
    {
        let r = s.shared.lock().unwrap();
        idle(&r)?;
        if !["Ready", "Busy", "Waiting for Approval"].contains(&r.view.lifecycle.as_str()) {
            return Err("Connect Claude first.".into());
        }
        r.view
            .thread_id
            .as_ref()
            .ok_or("Create or reopen a thread first")?;
    }
    {
        let mut r = s.shared.lock().unwrap();
        r.starting_turn = true;
        r.view.lifecycle = "Busy".into();
        r.view.error = None;
    }
    changed(s, app);
    let session = match ensure_claude_session(app, s).await {
        Ok(session) => session,
        Err(e) => {
            let mut r = s.shared.lock().unwrap();
            r.starting_turn = false;
            r.view.lifecycle = "Ready".into();
            r.view.error = Some(e.clone());
            r.changed(app);
            return Err(e);
        }
    };
    {
        let mut r = s.shared.lock().unwrap();
        let id = r.next_id("local-user");
        r.view
            .items
            .push(json!({"id": id, "type": "userMessage", "content": [{"type":"text","text": text}]}));
        let turn = r.next_id("claude-turn");
        r.view.turn_id = Some(turn);
        r.starting_turn = false;
        r.claude_has_history = true;
        r.claude_interrupting = false;
    }
    changed(s, app);
    if let Err(e) = session.user_message(&text).await {
        let mut r = s.shared.lock().unwrap();
        claude::end_turn(&mut r, true);
        r.view.error = Some(e.clone());
        r.changed(app);
        return Err(e);
    }
    Ok(())
}
#[tauri::command]
pub async fn interrupt(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    if provider(&s) == "claude" {
        let (session, generation) = {
            let mut r = s.shared.lock().unwrap();
            if r.view.turn_id.is_none() {
                return Err("No active turn to stop".into());
            }
            r.claude_interrupting = true;
            (r.claude.clone(), r.claude_gen)
        };
        match &session {
            Some(session) if session.alive() => {
                let _ = session.interrupt().await;
            }
            _ => {
                // Nothing is running any more; finish the turn now.
                let mut r = s.shared.lock().unwrap();
                claude::end_turn(&mut r, true);
                r.changed(&app);
                return Ok(());
            }
        }
        // If Claude does not confirm the stop quickly, end the process so the student is never stuck.
        let (shared, a) = (s.shared.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(8)).await;
            let stale = {
                let mut r = shared.lock().unwrap();
                if r.claude_gen == generation && r.claude_interrupting && r.view.turn_id.is_some()
                {
                    r.claude_gen += 1;
                    claude::end_turn(&mut r, true);
                    r.view.warning = Some("Claude was stopped.".into());
                    r.changed(&a);
                    r.claude.take()
                } else {
                    None
                }
            };
            if let Some(session) = stale {
                session.stop().await;
            }
        });
        changed(&s, &app);
        return Ok(());
    }
    let (rpc, thread, turn) = {
        let r = s.shared.lock().unwrap();
        (
            r.rpc()?,
            r.view.thread_id.clone().ok_or("No active thread")?,
            r.view
                .turn_id
                .clone()
                .ok_or("No active turn to interrupt yet")?,
        )
    };
    rpc.request("turn/interrupt", json!({"threadId":thread,"turnId":turn}))
        .await?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn decide(
    app: AppHandle,
    s: State<'_, AppState>,
    key: String,
    decision: String,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    let a = s
        .shared
        .lock()
        .unwrap()
        .pending
        .get(&key)
        .cloned()
        .ok_or("This request is no longer pending")?;
    if a.method == claude::APPROVAL_METHOD {
        let response = claude::decision_response(&a.params, &a.id, &decision)?;
        let session = s
            .shared
            .lock()
            .unwrap()
            .claude
            .clone()
            .ok_or("Claude stopped; this request was cancelled.")?;
        session.send(response).await?;
    } else {
        let rpc = rpc(&s)?;
        let response = protocol::approval_response(&a.method, &decision, &a.params)?;
        rpc.write(json!({"id":a.id,"result":response})).await?;
    }
    {
        let mut r = s.shared.lock().unwrap();
        r.pending.remove(&key);
        r.view.approvals = r.pending.values().cloned().collect();
        r.view.lifecycle = if r.pending.is_empty() {
            if r.view.turn_id.is_some() {
                "Busy"
            } else {
                "Ready"
            }
        } else {
            "Waiting for Approval"
        }
        .into();
    }
    changed(&s, &app);
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    theme: String,
    font_size: u8,
    model: Option<String>,
    sandbox: String,
    approval_policy: String,
    reasoning_effort: Option<String>,
    reasoning_summary: String,
    service_tier: Option<String>,
    layout: String,
    sidebar_width: u16,
    inspector_width: u16,
    debug_log: bool,
    claude_model: Option<String>,
    claude_effort: Option<String>,
    claude_project_settings: bool,
    composer_width: u16,
    composer_height: u16,
    show_inspector: bool,
    auto_connect: bool,
    proxy: String,
}
fn model_entry<'a>(models: &'a [Value], selected: &str) -> Option<&'a Value> {
    models
        .iter()
        .find(|m| m["id"].as_str() == Some(selected) || m["model"].as_str() == Some(selected))
}
#[tauri::command]
pub async fn save_preferences(
    app: AppHandle,
    s: State<'_, AppState>,
    prefs: Prefs,
) -> Result<(), String> {
    let p = prefs;
    {
        let mut guard = s.shared.lock().unwrap();
        let r: &mut Runtime = &mut guard;
        let c = &r.view.config;
        if !["dark", "light", "system"].contains(&p.theme.as_str())
            || !(11..=20).contains(&p.font_size)
            || !["read-only", "workspace-write", "danger-full-access"].contains(&p.sandbox.as_str())
            || !["on-request", "never"].contains(&p.approval_policy.as_str())
            || !["auto", "concise", "detailed", "none"].contains(&p.reasoning_summary.as_str())
            || !["bottom", "left", "right"].contains(&p.layout.as_str())
            || !(180..=420).contains(&p.sidebar_width)
            || !(220..=460).contains(&p.inspector_width)
            || !(260..=720).contains(&p.composer_width)
            || !(110..=520).contains(&p.composer_height)
        {
            return Err("Unsupported preference value".into());
        }
        // Permission changes apply to the next turn; refuse them mid-turn so nothing changes under it.
        if (p.sandbox != c.sandbox || p.approval_policy != c.approval_policy) && r.active() {
            return Err("Stop the current turn before changing what the AI may do.".into());
        }
        let proxy = p.proxy.trim().to_string();
        if !proxy.is_empty() && proxy != "none" {
            let url = url::Url::parse(&proxy).map_err(|_| {
                "Enter the proxy as a web address, for example http://proxy.school.edu:8080"
            })?;
            if !["http", "https", "socks5", "socks5h"].contains(&url.scheme())
                || url.host_str().is_none()
            {
                return Err("Enter the proxy as a web address, for example http://proxy.school.edu:8080".into());
            }
        }
        let codex_active = c.provider == "codex";
        if let Some(model) = &p.model {
            if codex_active && Some(model) != c.model.as_ref() && model_entry(&r.view.models, model).is_none() {
                return Err("Choose a model advertised by this Codex connection".into());
            }
        }
        let codex_model = p.model.clone().or(c.model.clone());
        if codex_active {
            if let Some(entry) = codex_model.as_deref().and_then(|m| model_entry(&r.view.models, m)) {
                if let Some(effort) = &p.reasoning_effort {
                    let ok = entry["supportedReasoningEfforts"]
                        .as_array()
                        .map(|v| v.iter().any(|x| x["reasoningEffort"].as_str() == Some(effort)))
                        .unwrap_or(true);
                    if !ok {
                        return Err("Choose a reasoning effort advertised by this model".into());
                    }
                }
                if let Some(tier) = &p.service_tier {
                    let ok = entry["serviceTiers"]
                        .as_array()
                        .map(|v| v.iter().any(|x| x["id"].as_str() == Some(tier)))
                        .unwrap_or(true);
                    if !ok {
                        return Err("Choose a service tier advertised by this model".into());
                    }
                }
            }
        }
        if let Some(model) = &p.claude_model {
            let plain = model.len() <= 80
                && model
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "-._[]".contains(ch));
            if !plain {
                return Err("Choose a Claude model from the list".into());
            }
        }
        if let Some(effort) = &p.claude_effort {
            if !CLAUDE_EFFORTS.contains(&effort.as_str()) {
                return Err("Choose a Claude effort level from the list".into());
            }
        }
        let c = &mut r.view.config;
        c.theme = p.theme;
        c.font_size = p.font_size;
        c.model = p.model;
        c.sandbox = p.sandbox;
        c.approval_policy = p.approval_policy;
        c.reasoning_effort = p.reasoning_effort;
        c.reasoning_summary = p.reasoning_summary;
        c.service_tier = p.service_tier;
        c.layout = p.layout;
        c.sidebar_width = p.sidebar_width;
        c.inspector_width = p.inspector_width;
        c.debug_log = p.debug_log;
        c.claude_model = p.claude_model;
        c.claude_effort = p.claude_effort;
        c.claude_project_settings = p.claude_project_settings;
        c.composer_width = p.composer_width;
        c.composer_height = p.composer_height;
        c.show_inspector = p.show_inspector;
        c.auto_connect = p.auto_connect;
        c.proxy = proxy;
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn set_ca_certificate(
    app: AppHandle,
    s: State<'_, AppState>,
    clear: bool,
) -> Result<(), String> {
    let path = if clear {
        None
    } else {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Choose your school's security certificate")
            .add_filter("Certificate", &["pem", "crt", "cer"])
            .pick_file()
            .await
        else {
            return Ok(());
        };
        let text = std::fs::read_to_string(file.path()).map_err(|_| {
            "This certificate is not in text (PEM) format. Ask IT for a .pem or Base-64 .cer file."
        })?;
        if !text.contains("-----BEGIN CERTIFICATE-----") {
            return Err("This certificate is not in text (PEM) format. Ask IT for a .pem or Base-64 .cer file.".into());
        }
        Some(crate::portable_paths::display_path(file.path()))
    };
    s.shared.lock().unwrap().view.config.ca_certificate = path;
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn choose_binary(
    app: AppHandle,
    s: State<'_, AppState>,
    provider: Option<String>,
) -> Result<(), String> {
    {
        idle(&s.shared.lock().unwrap())?;
    }
    let claude = provider.as_deref() == Some("claude");
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(if claude {
            "Select the official Claude Code program (claude.exe)"
        } else {
            "Select official Codex 0.163.0 x64 executable"
        })
        .add_filter("Windows program", &["exe"])
        .pick_file()
        .await
    else {
        return Ok(());
    };
    if claude {
        let info = claude::validate(file.path().to_path_buf()).await?;
        let _op = s.operation.lock().await;
        {
            let mut r = s.shared.lock().unwrap();
            r.view.config.claude_binary_path = Some(info.path.clone());
            r.view.claude_binary = Some(info);
            let engines = engines(&s.paths, &r.view.config);
            r.view.engines = engines;
        }
    } else {
        let info = validate_binary(file.path().to_path_buf()).await?;
        let _op = s.operation.lock().await;
        {
            let mut r = s.shared.lock().unwrap();
            idle(&r)?;
            r.view.config.binary_path = Some(info.path.clone());
            r.view.binary = Some(info);
            let engines = engines(&s.paths, &r.view.config);
            r.view.engines = engines;
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
fn login_handler(
    s: &AppState,
    app: &AppHandle,
    generation: u64,
    binary: PathBuf,
    env: netenv::ChildEnv,
    cwd: PathBuf,
) -> claude::Handler {
    let shared = s.shared.clone();
    let paths = s.paths.clone();
    let a = app.clone();
    Arc::new(move |frame| {
        let mut r = shared.lock().unwrap();
        if r.login_gen != generation {
            return;
        }
        if frame.get("loginUpdate").is_some() {
            r.view.login = json!({"provider":"claude","authUrl":frame["url"],"output":frame["output"],"acceptsCode":true});
        } else if frame.get("loginExit").is_some() {
            r.claude_login = None;
            r.view.login = Value::Null;
            if frame["loginExit"] == 0 {
                r.view.warning = Some("Claude sign-in finished.".into());
                let (shared, paths, a, binary, env, cwd) = (
                    shared.clone(),
                    paths.clone(),
                    a.clone(),
                    binary.clone(),
                    env.clone(),
                    cwd.clone(),
                );
                tauri::async_runtime::spawn(async move {
                    if let Ok(status) = claude::auth_status(&binary, &env, &cwd).await {
                        let dirty = {
                            let mut guard = shared.lock().unwrap();
                            let r: &mut Runtime = &mut guard;
                            apply_claude_status(r, &status);
                            r.changed(&a);
                            std::mem::take(&mut r.config_dirty).then(|| r.view.config.clone())
                        };
                        if let Some(c) = dirty {
                            let _ = paths.save(&c);
                        }
                    }
                });
            } else {
                r.view.error = Some(format!(
                    "Claude sign-in did not finish. {}",
                    frame["output"].as_str().unwrap_or("")
                ));
            }
        }
        r.changed(&a);
    })
}
#[tauri::command]
pub async fn auth(
    app: AppHandle,
    s: State<'_, AppState>,
    action: String,
    code: Option<String>,
) -> Result<(), String> {
    if provider(&s) == "claude" {
        return claude_auth(&app, &s, &action, code).await;
    }
    let _op = s.operation.lock().await;
    let rpc = rpc(&s)?;
    let value = match action.as_str() {
        "status" => {
            let generation = s.shared.lock().unwrap().view.generation;
            refresh_codex_account(&s.shared, &s.paths, &app, &rpc, generation).await;
            Value::Null
        }
        "device" | "browser" => {
            let mut v = rpc
                .request(
                    "account/login/start",
                    json!({"type":if action=="device"{"chatgptDeviceCode"}else{"chatgpt"}}),
                )
                .await?;
            v["provider"] = json!("codex");
            v
        }
        "cancel" => {
            let id = s.shared.lock().unwrap().view.login["loginId"].clone();
            rpc.request("account/login/cancel", json!({"loginId":id}))
                .await?;
            Value::Null
        }
        "logout" => {
            idle(&s.shared.lock().unwrap())?;
            rpc.request("account/logout", json!({})).await?;
            apply_codex_account(&mut s.shared.lock().unwrap(), json!({"account": null}));
            save_if_dirty(&s);
            Value::Null
        }
        "sandbox" => {
            idle(&s.shared.lock().unwrap())?;
            rpc.request(
                "windowsSandbox/setupStart",
                json!({"mode":"unelevated","cwd":project(&s)?}),
            )
            .await?;
            Value::Null
        }
        _ => return Err("Unknown account action".into()),
    };
    if ["device", "browser", "cancel"].contains(&action.as_str()) {
        s.shared.lock().unwrap().view.login = value;
    }
    changed(&s, &app);
    Ok(())
}
async fn claude_auth(
    app: &AppHandle,
    s: &AppState,
    action: &str,
    code: Option<String>,
) -> Result<(), String> {
    let binary = claude_binary(s)?;
    let env = claude_env(s)?;
    let cwd = work_dir(s);
    match action {
        "status" => {
            let status = claude::auth_status(&binary, &env, &cwd).await?;
            apply_claude_status(&mut s.shared.lock().unwrap(), &status);
            save_if_dirty(s);
        }
        "browser" | "console" => {
            let generation = {
                let mut r = s.shared.lock().unwrap();
                if let Some(old) = r.claude_login.take() {
                    old.cancel();
                }
                r.login_gen += 1;
                r.view.login = json!({"provider":"claude","authUrl":null,"output":"Starting Claude sign-in…","acceptsCode":true});
                r.login_gen
            };
            let handler = login_handler(s, app, generation, binary.clone(), env.clone(), cwd.clone());
            let login =
                claude::Login::start(&binary, &env, &cwd, action == "console", handler).await?;
            let mut r = s.shared.lock().unwrap();
            if r.login_gen == generation {
                r.claude_login = Some(login);
            } else {
                login.cancel();
            }
        }
        "code" => {
            let code = code.unwrap_or_default();
            if code.trim().is_empty() || code.len() > 4096 {
                return Err("Paste the code shown in the browser".into());
            }
            let sender = {
                let r = s.shared.lock().unwrap();
                r.claude_login.as_ref().map(|l| l.stdin_sender())
            };
            let sender = sender.ok_or("Sign-in is not running. Start it again.")?;
            sender
                .send(code.trim().to_string())
                .await
                .map_err(|_| "Sign-in already finished".to_string())?;
        }
        "cancel" => {
            let mut r = s.shared.lock().unwrap();
            r.login_gen += 1;
            if let Some(login) = r.claude_login.take() {
                login.cancel();
            }
            r.view.login = Value::Null;
        }
        "logout" => {
            let _op = s.operation.lock().await;
            {
                idle(&s.shared.lock().unwrap())?;
            }
            let session = {
                let mut r = s.shared.lock().unwrap();
                r.claude_gen += 1;
                r.claude.take()
            };
            if let Some(session) = session {
                session.stop().await;
            }
            claude::logout(&binary, &env, &cwd).await?;
            apply_claude_status(&mut s.shared.lock().unwrap(), &json!({"loggedIn": false}));
            save_if_dirty(s);
        }
        _ => return Err("Use browser sign-in for Claude.".into()),
    }
    changed(s, app);
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Accounts

fn safe_remove_dir(path: &Path, inside: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    let parent = inside.canonicalize().map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || !canonical.starts_with(&parent) || canonical == parent
    {
        return Err(
            "Account folder is a link or lies outside the portable data folder; refusing removal."
                .into(),
        );
    }
    std::fs::remove_dir_all(path).map_err(|e| {
        format!("Could not remove the account folder ({e}). Close other programs using it and try again.")
    })
}
#[tauri::command]
pub async fn add_account(
    app: AppHandle,
    s: State<'_, AppState>,
    provider: String,
    label: String,
) -> Result<String, String> {
    if !["codex", "claude"].contains(&provider.as_str()) {
        return Err("Unknown AI provider".into());
    }
    let label = label.trim().chars().take(60).collect::<String>();
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    let account = {
        let r = s.shared.lock().unwrap();
        if r.view.config.accounts.len() >= 24 {
            return Err("This app can keep up to 24 accounts.".into());
        }
        let mut n = 2;
        while r
            .view
            .config
            .accounts
            .iter()
            .any(|a| a.id == format!("{provider}-{n}"))
        {
            n += 1;
        }
        let name = if label.is_empty() {
            format!(
                "{} account {n}",
                if provider == "codex" { "ChatGPT" } else { "Claude" }
            )
        } else {
            label
        };
        Account::new(&format!("{provider}-{n}"), &provider, &name)
    };
    s.paths.account_home(&account)?;
    let current = self::provider(&s) == provider;
    if current {
        stop_engines(&s).await;
    }
    {
        let mut r = s.shared.lock().unwrap();
        r.view.config.accounts.push(account.clone());
        if provider == "codex" {
            r.view.config.active_codex_account = account.id.clone();
        } else {
            r.view.config.active_claude_account = account.id.clone();
        }
        if current {
            reset_view(&mut r, "Stopped");
            r.view.config.selected_thread = None;
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(account.id)
}
#[tauri::command]
pub async fn switch_account(
    app: AppHandle,
    s: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    let account = config(&s)
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Unknown account")?;
    let current = provider(&s) == account.provider;
    let already = config(&s).active_account(&account.provider).id == id;
    if already {
        return Ok(());
    }
    if current {
        stop_engines(&s).await;
    }
    {
        let mut r = s.shared.lock().unwrap();
        if account.provider == "codex" {
            r.view.config.active_codex_account = id;
        } else {
            r.view.config.active_claude_account = id;
        }
        if current {
            reset_view(&mut r, "Stopped");
            r.view.config.selected_thread = None;
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn rename_account(
    app: AppHandle,
    s: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<(), String> {
    let label = label.trim().chars().take(60).collect::<String>();
    if label.is_empty() {
        return Err("Enter a name for the account".into());
    }
    {
        let mut r = s.shared.lock().unwrap();
        let account = r
            .view
            .config
            .account_mut(&id)
            .ok_or("Unknown account")?;
        account.label = label;
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn remove_account(
    app: AppHandle,
    s: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    if id == CODEX_DEFAULT || id == CLAUDE_DEFAULT || !valid_account_id(&id) {
        return Err("The first account of each kind cannot be removed. Use Sign out instead.".into());
    }
    let account = config(&s)
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Unknown account")?;
    let confirmed = rfd::AsyncMessageDialog::new()
        .set_title("Remove this account?")
        .set_description(format!("\"{}\" will be signed out on this computer, and its saved chats in this app folder will be deleted. Your project files are not touched. Continue?", account.label))
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    if confirmed != rfd::MessageDialogResult::Yes {
        return Ok(());
    }
    let _op = s.operation.lock().await;
    {
        idle(&s.shared.lock().unwrap())?;
    }
    let active = config(&s).active_account(&account.provider).id == id;
    if active && provider(&s) == account.provider {
        stop_engines(&s).await;
    }
    if account.provider == "claude" {
        // Clears the sign-in wherever Claude keeps it (file or Windows Credential Manager).
        if let (Ok(binary), Ok(env)) = (claude_binary(&s), claude_env_for(&s, &account)) {
            let _ = claude::logout(&binary, &env, &work_dir(&s)).await;
        }
    }
    let home = s.paths.account_home(&account)?;
    safe_remove_dir(&home, &s.paths.data.join("accounts"))?;
    {
        let mut r = s.shared.lock().unwrap();
        r.view.config.accounts.retain(|a| a.id != id);
        if active {
            if account.provider == "codex" {
                r.view.config.active_codex_account = CODEX_DEFAULT.into();
            } else {
                r.view.config.active_claude_account = CLAUDE_DEFAULT.into();
            }
            if r.view.config.provider == account.provider {
                reset_view(&mut r, "Stopped");
            }
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
/// Re-reads usage for every ChatGPT account (a short, read-only Codex start for inactive ones) and
/// the sign-in email for Claude accounts. Claude only reports usage while it is answering.
#[tauri::command]
pub async fn refresh_usage(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let c = config(&s);
    let live_codex = {
        let r = s.shared.lock().unwrap();
        (r.view.config.provider == "codex")
            .then(|| r.rpc().ok())
            .flatten()
    };
    let codex_accounts: Vec<Account> = c
        .accounts
        .iter()
        .filter(|a| a.provider == "codex")
        .cloned()
        .collect();
    let codex_binary = binary(&s);
    let codex_ok = !codex_accounts.is_empty()
        && codex_binary.is_file()
        && validate_binary(codex_binary.clone()).await.is_ok();
    for account in codex_accounts {
        let active = c.active_codex_account == account.id;
        let result = if let (true, Some(rpc)) = (active, live_codex.clone()) {
            let a = rpc
                .request("account/read", json!({"refreshToken":false}))
                .await;
            let l = rpc
                .request(
                    "account/rateLimits/read",
                    json!({"excludeResetCreditDetails":true}),
                )
                .await;
            a.and_then(|a| l.map(|l| (a, l)))
        } else if codex_ok {
            probe_codex(&s, &account, &codex_binary).await
        } else {
            continue;
        };
        if let Ok((a, l)) = result {
            let mut r = s.shared.lock().unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(acc) = r.view.config.account_mut(&account.id) {
                acc.email = a["account"]["email"].as_str().map(str::to_string);
                acc.plan = a["account"]["planType"].as_str().map(str::to_string);
                acc.usage = Some(codex_usage(&l["rateLimits"]));
                acc.usage_at = Some(now);
            }
        }
        changed(&s, &app);
    }
    if let Ok(binary) = claude_binary(&s) {
        for account in c.accounts.iter().filter(|a| a.provider == "claude") {
            let Ok(env) = claude_env_for(&s, account) else {
                continue;
            };
            if let Ok(status) = claude::auth_status(&binary, &env, &work_dir(&s)).await {
                let info = claude::account_from_status(&status);
                let mut r = s.shared.lock().unwrap();
                if let Some(acc) = r.view.config.account_mut(&account.id) {
                    acc.email = info["account"]["email"].as_str().map(str::to_string);
                    acc.plan = info["account"]["planType"].as_str().map(str::to_string);
                }
            }
        }
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
async fn probe_codex(
    s: &AppState,
    account: &Account,
    binary: &Path,
) -> Result<(Value, Value), String> {
    let home = s.paths.account_home(account)?;
    if !home.join("auth.json").is_file() {
        return Err("Not signed in".into());
    }
    let handler: EventHandler = Arc::new(|_| {});
    let rpc = Rpc::spawn(
        binary,
        &home,
        &s.paths.data.join("temp"),
        &{
            let mut env = netenv::codex_env(&config(s), Some(&s.paths.data.join("temp")));
            env.extend(&tools::env(&layout(&s.paths)));
            env
        },
        handler,
    )
    .await?;
    let result = async {
        rpc.handshake().await?;
        let a = rpc
            .request("account/read", json!({"refreshToken":false}))
            .await?;
        let l = rpc
            .request(
                "account/rateLimits/read",
                json!({"excludeResetCreditDetails":true}),
            )
            .await?;
        Ok::<_, String>((a, l))
    }
    .await;
    rpc.stop().await;
    result
}

// ------------------------------------------------------------------------------------------------
// Programming tools, new projects, Run and Terminal

fn job_report(shared: &Shared, app: &AppHandle, id: &'static str, prefix: String) -> tools::Report {
    let (shared, app) = (shared.clone(), app.clone());
    Arc::new(move |percent: Option<u8>, message: &str, line: Option<&str>| {
        let mut r = shared.lock().unwrap();
        r.job(id, |j| {
            match (percent, line) {
                (Some(p), _) => j.percent = Some(p),
                (None, None) => j.percent = None,
                _ => {}
            }
            j.message = if prefix.is_empty() { message.to_string() } else { format!("{prefix}: {message}") };
            if let Some(l) = line {
                j.log.push(l.chars().take(300).collect());
            }
        });
        r.changed(&app);
    })
}
fn job_running(r: &Runtime, id: &str) -> bool {
    r.view.jobs.iter().any(|j| j.id == id && j.state == "running")
}
#[tauri::command]
pub async fn install_tools(app: AppHandle, s: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    let ids: Vec<&'static str> = tools::TOOLS.iter().map(|t| t.id).filter(|id| ids.iter().any(|x| x == id)).collect();
    if ids.is_empty() {
        return Err("Choose a tool to get.".into());
    }
    {
        let mut r = s.shared.lock().unwrap();
        if job_running(&r, "tools") {
            return Err("A download is already running. Wait for it to finish.".into());
        }
        let names: Vec<&str> = ids.iter().filter_map(|id| tools::TOOLS.iter().find(|t| t.id == *id).map(|t| t.name)).collect();
        r.job("tools", |j| {
            *j = Job { id: "tools".into(), title: format!("Getting {}", names.join(", ")), state: "running".into(), message: "Starting…".into(), ..Default::default() };
        });
        r.changed(&app);
    }
    let (shared, paths) = (s.shared.clone(), s.paths.clone());
    let net = netenv::network_env(&config(&s), Some(&s.paths.data.join("temp")));
    tauri::async_runtime::spawn(async move {
        let l = layout(&paths);
        let mut failed = vec![];
        for id in &ids {
            let name = tools::TOOLS.iter().find(|t| t.id == *id).map(|t| t.name).unwrap_or(id);
            let report = job_report(&shared, &app, "tools", name.to_string());
            match tools::install(&l, id, &net, &report).await {
                Ok(()) => report(Some(100), "ready", Some(&format!("{name} is ready."))),
                Err(e) => {
                    report(None, "failed", Some(&e));
                    failed.push(format!("{name}: {e}"));
                }
            }
        }
        let mut r = shared.lock().unwrap();
        r.job("tools", |j| {
            j.percent = Some(100);
            if failed.is_empty() {
                j.state = "done".into();
                j.message = "All set. New tools are ready to use.".into();
            } else {
                j.state = "failed".into();
                j.message = failed.join("\n");
            }
        });
        refresh_env_info(&paths, &mut r);
        r.changed(&app);
    });
    Ok(())
}
#[tauri::command]
pub async fn remove_tool(app: AppHandle, s: State<'_, AppState>, id: String) -> Result<(), String> {
    let tool = tools::TOOLS.iter().find(|t| t.id == id).ok_or("Unknown tool")?;
    let answer = rfd::AsyncMessageDialog::new()
        .set_title("Remove this tool?")
        .set_description(format!("Remove {} from the app folder to free about {} MB? Your projects are not touched; you can get the tool again later.", tool.name, tool.disk_mb))
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    if answer != rfd::MessageDialogResult::Yes {
        return Ok(());
    }
    if job_running(&s.shared.lock().unwrap(), "tools") {
        return Err("Wait for the current download to finish.".into());
    }
    let l = layout(&s.paths);
    let result = tokio::task::spawn_blocking(move || tools::remove(&l, &id)).await.map_err(|e| e.to_string())?;
    refresh_env_info(&s.paths, &mut s.shared.lock().unwrap());
    changed(&s, &app);
    result
}
#[tauri::command]
pub async fn choose_folder() -> Result<Option<String>, String> {
    Ok(rfd::AsyncFileDialog::new()
        .set_title("Where should the new project folder go?")
        .pick_folder()
        .await
        .map(|f| crate::portable_paths::display_path(f.path())))
}
#[tauri::command]
pub async fn create_project(
    app: AppHandle,
    s: State<'_, AppState>,
    name: String,
    template: String,
    parent: Option<String>,
    git: bool,
) -> Result<(), String> {
    let folder = scaffold::folder_name(&name)?;
    if !scaffold::TEMPLATES.iter().any(|t| t.id == template) {
        return Err("Choose a project type.".into());
    }
    let l = layout(&s.paths);
    let parent = parent.filter(|p| !p.trim().is_empty()).map(PathBuf::from).unwrap_or_else(|| l.projects());
    {
        let mut r = s.shared.lock().unwrap();
        if job_running(&r, "project") {
            return Err("A project is already being set up.".into());
        }
        r.job("project", |j| {
            *j = Job { id: "project".into(), title: format!("Creating {folder}"), state: "running".into(), percent: Some(0), message: "Starting…".into(), ..Default::default() };
        });
        r.changed(&app);
    }
    let env = tool_env(&s);
    let (shared, paths) = (s.shared.clone(), s.paths.clone());
    tauri::async_runtime::spawn(async move {
        let report = job_report(&shared, &app, "project", String::new());
        let result = scaffold::create(&l, &parent, &name, &template, git, &env, &report).await;
        let dir = parent.join(&folder);
        // Even if a setup step failed, the folder exists: keep it in the list so it can be opened.
        let path = canonical_project(&dir.to_string_lossy()).ok();
        let mut r = shared.lock().unwrap();
        if let Some(path) = &path {
            if !r.view.config.projects.contains(path) {
                r.view.config.projects.push(path.clone());
                let c = r.view.config.clone();
                let _ = paths.save(&c);
            }
        }
        r.job("project", |j| {
            j.result = path.clone();
            j.percent = Some(100);
            match &result {
                Ok(_) => {
                    j.state = "done".into();
                    j.message = "Your project is ready.".into();
                }
                Err(e) => {
                    j.state = "failed".into();
                    j.message = e.clone();
                }
            }
        });
        r.changed(&app);
    });
    Ok(())
}
#[tauri::command]
pub async fn run_project(s: State<'_, AppState>) -> Result<(), String> {
    let p = project(&s)?;
    let l = layout(&s.paths);
    let (label, command, tool) = tools::runner(Path::new(&p)).ok_or(
        "The app doesn't know how to run this project yet. Ask the AI to add a main.py, Cargo.toml or package.json, or use Terminal.",
    )?;
    if !tools::usable(&l, tool) {
        let name = tools::TOOLS.iter().find(|t| t.id == tool).map(|t| t.name).unwrap_or(tool);
        return Err(format!("{name} is not set up yet. Open Programming tools to get it."));
    }
    let name = Path::new(&p).file_name().unwrap_or_default().to_string_lossy().to_string();
    tools::open_console(&l, Path::new(&p), &format!("{name} - {label}"), Some(&command), &tool_env(&s))
}
#[tauri::command]
pub async fn open_terminal(s: State<'_, AppState>) -> Result<(), String> {
    let p = project(&s)?;
    let l = layout(&s.paths);
    let name = Path::new(&p).file_name().unwrap_or_default().to_string_lossy().to_string();
    tools::open_console(&l, Path::new(&p), &name, None, &tool_env(&s))
}
/// Restarts the AI so it sees newly added tools, reopening the chat that was open.
#[tauri::command]
pub async fn restart_ai(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let thread = s.shared.lock().unwrap().view.thread_id.clone();
    connect(app.clone(), s.clone()).await?;
    if let Some(id) = thread {
        let _ = open_thread(app, s, Some(id), false).await;
    }
    Ok(())
}
/// Saves a ZIP of this app folder with its tools but without anyone's sign-in, chats or settings,
/// so a teacher can prepare the tools once and hand the same copy to every student.
#[tauri::command]
pub async fn export_classroom_copy(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Save a classroom copy (without your sign-in)")
        .set_file_name("RustPortableHarness-classroom.zip")
        .add_filter("ZIP file", &["zip"])
        .save_file()
        .await
    else {
        return Ok(());
    };
    let dest = file.path().to_path_buf();
    let root = s.paths.root.clone();
    if dest.starts_with(&root) {
        return Err("Save the classroom copy outside the app folder (for example on the Desktop).".into());
    }
    {
        let mut r = s.shared.lock().unwrap();
        if job_running(&r, "export") {
            return Err("A classroom copy is already being made.".into());
        }
        r.job("export", |j| {
            *j = Job { id: "export".into(), title: "Making a classroom copy".into(), state: "running".into(), percent: Some(0), message: "Collecting files…".into(), ..Default::default() };
        });
        r.changed(&app);
    }
    let shared = s.shared.clone();
    tauri::async_runtime::spawn(async move {
        let report = job_report(&shared, &app, "export", String::new());
        let d2 = dest.clone();
        let result = tokio::task::spawn_blocking(move || write_classroom_zip(&root, &d2, &report)).await.map_err(|e| e.to_string()).and_then(|r| r);
        let mut r = shared.lock().unwrap();
        r.job("export", |j| {
            j.percent = Some(100);
            match &result {
                Ok(()) => {
                    j.state = "done".into();
                    j.message = format!("Saved {}. Give students this ZIP; they extract it and sign in with their own accounts.", dest.display());
                    j.result = Some(dest.to_string_lossy().to_string());
                }
                Err(e) => {
                    j.state = "failed".into();
                    j.message = e.clone();
                    let _ = std::fs::remove_file(&dest);
                }
            }
        });
        r.changed(&app);
    });
    Ok(())
}
fn write_classroom_zip(root: &Path, dest: &Path, report: &tools::Report) -> Result<(), String> {
    use std::io::Write;
    let mut files = vec![];
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let path = e.path();
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf();
            let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
            // data/ holds sign-ins, chats and settings; projects/ holds the teacher's own work.
            if top == "data" || top == "projects" {
                continue;
            }
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(path);
            } else if !path.to_string_lossy().ends_with(".part") {
                total += e.metadata().map(|m| m.len()).unwrap_or(0);
                files.push((path, rel));
            }
        }
    }
    let out = std::fs::File::create(dest).map_err(|e| format!("Cannot create {}: {e}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(out));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    for name in ["data/", "projects/"] {
        zip.add_directory(format!("RustPortableHarness/{name}"), options).map_err(|e| e.to_string())?;
    }
    let mut done = 0u64;
    let mut last = 0u8;
    let mut buf = vec![0u8; 1 << 20];
    for (path, rel) in files {
        let name = format!("RustPortableHarness/{}", rel.to_string_lossy().replace('\\', "/"));
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        let mut f = std::fs::File::open(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        loop {
            let n = std::io::Read::read(&mut f, &mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n]).map_err(|e| format!("Cannot write the ZIP ({e}). Is the disk full?"))?;
            done += n as u64;
        }
        let pct = ((done * 100) / total.max(1)).min(99) as u8;
        if pct != last {
            last = pct;
            report(Some(pct), &format!("Packing {} of {} MB…", done >> 20, total >> 20), None);
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn diagnostics(s: State<'_, AppState>) -> Result<Value, String> {
    Ok(diagnostic_value(&s))
}
#[tauri::command]
pub async fn wipe_account(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let c = config(&s);
    let account = c.active_account(&c.provider);
    let confirmed = rfd::AsyncMessageDialog::new()
        .set_title("Wipe this account's sign-in and history?")
        .set_description(format!("This stops the AI and permanently removes the sign-in, settings and saved chats of \"{}\" from this app folder. Your projects and app preferences are kept. Continue?", account.label))
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    if confirmed != rfd::MessageDialogResult::Yes {
        return Ok(());
    }
    let _op = s.operation.lock().await;
    let (rpc, session) = {
        let mut r = s.shared.lock().unwrap();
        r.view.generation += 1;
        r.claude_gen += 1;
        r.view.lifecycle = "Stopped".into();
        r.pending.clear();
        r.view.approvals.clear();
        r.view.turn_id = None;
        r.starting_turn = false;
        (r.rpc.take(), r.claude.take())
    };
    if let Some(rpc) = rpc {
        rpc.stop().await;
        if !rpc.has_exited() {
            return Err("Codex has not stopped yet. Try again after it exits.".into());
        }
    }
    if let Some(session) = session {
        session.stop().await;
    }
    if account.provider == "claude" {
        if let (Ok(binary), Ok(env)) = (claude_binary(&s), claude_env_for(&s, &account)) {
            let _ = claude::logout(&binary, &env, &work_dir(&s)).await;
        }
    }
    let home = s.paths.account_home(&account)?;
    safe_remove_dir(&home, &s.paths.data)?;
    s.paths.account_home(&account)?;
    {
        let mut r = s.shared.lock().unwrap();
        reset_view(&mut r, "Stopped");
        r.view.config.selected_thread = None;
        if let Some(a) = r.view.config.account_mut(&account.id) {
            a.email = None;
            a.plan = None;
            a.usage = None;
            a.usage_at = None;
        }
        r.view.warning =
            Some("The account's sign-in and history were wiped. Connect to sign in again.".into());
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
fn diagnostic_value(s: &AppState) -> Value {
    let r = s.shared.lock().unwrap();
    let c = &r.view.config;
    let system_root = std::env::var_os("SystemRoot").map(PathBuf::from);
    let powershell = system_root
        .as_ref()
        .map(|root| root.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
        .map(|p| p.exists())
        .unwrap_or(false);
    let cmd = system_root
        .as_ref()
        .map(|root| root.join("System32\\cmd.exe"))
        .map(|p| p.exists())
        .unwrap_or(false);
    let free_mb = netenv::free_space(&s.paths.root).map(|b| b / (1024 * 1024));
    json!({
        "appVersion": env!("CARGO_PKG_VERSION"),
        "provider": c.provider,
        "lifecycle": r.view.lifecycle,
        "codexExpected": protocol::CODEX_VERSION,
        "binary": r.view.binary,
        "claudeBinary": r.view.claude_binary,
        "claudeSession": r.view.claude_info,
        "engines": r.view.engines,
        "tools": r.view.tools,
        "root": s.paths.root,
        "freeSpaceMB": free_mb,
        "accounts": c.accounts.iter().map(|a| json!({"id": a.id, "provider": a.provider, "signedIn": a.email.is_some(), "usageKnown": a.usage.is_some()})).collect::<Vec<_>>(),
        "network": netenv::proxy_summary(c),
        "customCertificate": c.ca_certificate.is_some(),
        "notices": r.view.notices,
        "webview2": "Evergreen runtime required; browser profile is under data/webview2",
        "sandbox": c.sandbox,
        "approvalPolicy": c.approval_policy,
        "reasoningEffort": c.reasoning_effort,
        "serviceTier": c.service_tier,
        "claudeProjectSettings": c.claude_project_settings,
        "authStorage": "Each account keeps its sign-in in its own folder under data/; never included in diagnostics",
        "shells": {"powershell": powershell, "cmd": cmd, "runtimeRequirement": "The harness starts codex.exe/claude.exe directly; shells only matter for commands the AI runs."},
        "platform": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
    })
}
#[tauri::command]
pub async fn export_diagnostics(s: State<'_, AppState>) -> Result<String, String> {
    let path = s.paths.data.join("logs/diagnostics.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&diagnostic_value(&s)).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into())
}
#[tauri::command]
pub async fn open_external(url: String) -> Result<(), String> {
    let url = url::Url::parse(&url).map_err(|_| "Invalid URL")?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err("Only HTTPS web destinations can be opened".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url.as_str())
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_project_folder(s: State<'_, AppState>, path: String) -> Result<(), String> {
    let canonical = canonical_project(&path)?;
    if !s
        .shared
        .lock()
        .unwrap()
        .view
        .config
        .projects
        .contains(&canonical)
    {
        return Err("Choose this folder using Add folder first".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .arg(&canonical)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Could not open the project folder: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&canonical)
            .spawn()
            .map_err(|e| format!("Could not open the project folder: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classroom_copy_leaves_out_sign_ins_and_projects() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("app");
        for (file, body) in [
            ("RustPortableHarness.exe", "exe"),
            ("tools/python/python.exe", "py"),
            ("tools/node/node.exe", "node"),
            ("data/codex-home/auth.json", "SECRET"),
            ("data/config.json", "{}"),
            ("projects/my-game/main.py", "teacher work"),
            ("data/downloads/x.part", "partial"),
        ] {
            let p = root.join(file);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let dest = t.path().join("classroom.zip");
        let report: tools::Report = Arc::new(|_, _, _| {});
        write_classroom_zip(&root, &dest, &report).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"RustPortableHarness/tools/python/python.exe".to_string()));
        assert!(names.contains(&"RustPortableHarness/RustPortableHarness.exe".to_string()));
        assert!(names.contains(&"RustPortableHarness/data/".to_string()));
        assert!(!names.iter().any(|n| n.contains("auth.json") || n.contains("config.json") || n.contains("main.py") || n.ends_with(".part")), "{names:?}");
    }
}
