use crate::{
    portable_paths::{canonical_project, Config, PortablePaths},
    protocol,
    rpc_transport::{EventHandler, Rpc},
    state::{BinaryInfo, Shared, Snapshot},
};
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
}
fn idle(r: &crate::state::Runtime) -> Result<(), String> {
    if r.active() {
        Err(
            "Stop the current turn before changing projects, threads, or connection settings."
                .into(),
        )
    } else {
        Ok(())
    }
}
fn save(s: &AppState) -> Result<(), String> {
    s.paths.save(&s.shared.lock().unwrap().view.config)
}
fn changed(s: &AppState, app: &AppHandle) {
    s.shared.lock().unwrap().changed(app);
}
fn rpc(s: &AppState) -> Result<Rpc, String> {
    s.shared.lock().unwrap().rpc()
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
pub async fn validate_binary(path: PathBuf) -> Result<BinaryInfo, String> {
    let p = path.clone();
    let hash=tokio::task::spawn_blocking(move||->Result<String,String>{use sha2::{Digest,Sha256};use std::io::{Read,Seek,SeekFrom};let mut f=std::fs::File::open(&p).map_err(|e|format!("Codex executable missing or unreadable: {e}. Choose an official binary in Settings."))?;let mut dos=[0u8;64];f.read_exact(&mut dos).map_err(|_|"Invalid Codex executable")?;if &dos[..2]!=b"MZ"{return Err("Codex must be a Windows x64 executable".into());}let offset=u32::from_le_bytes(dos[60..64].try_into().unwrap());f.seek(SeekFrom::Start(offset as u64)).map_err(|e|e.to_string())?;let mut pe=[0u8;6];f.read_exact(&mut pe).map_err(|e|e.to_string())?;if &pe[..4]!=b"PE\0\0"||pe[4..]!=[0x64,0x86]{return Err("Codex must have a Windows x64 PE header".into());}f.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;let mut hasher=Sha256::new();let mut buf=[0u8;65536];loop{let n=f.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break;}hasher.update(&buf[..n]);}Ok(hex::encode(hasher.finalize()))}).await.map_err(|e|e.to_string())??;
    let mut c = tokio::process::Command::new(&path);
    c.arg("--version").kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    let out = tokio::time::timeout(Duration::from_secs(10), c.output())
        .await
        .map_err(|_| "Codex --version timed out")?
        .map_err(|e| e.to_string())?;
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
    let old = {
        let mut r = s.shared.lock().unwrap();
        r.view.generation += 1;
        r.view.lifecycle = "Starting".into();
        r.view.error = None;
        r.pending.clear();
        r.view.approvals.clear();
        r.rpc.take()
    };
    if let Some(old) = old {
        old.stop().await;
    }
    changed(&s, &app);
    let result = connect_inner(&app, &s).await;
    if let Err(e) = &result {
        s.shared.lock().unwrap().fail(e.clone());
        changed(&s, &app);
    }
    result
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
        s.paths.home.clone()
    };
    let generation = {
        let mut r = s.shared.lock().unwrap();
        r.view.binary = Some(info.clone());
        r.view.lifecycle = "Handshaking".into();
        r.view.generation
    };
    changed(s, app);
    let shared = s.shared.clone();
    let a = app.clone();
    let cell: Arc<Mutex<Option<Rpc>>> = Default::default();
    let out = cell.clone();
    let log_path = s.paths.data.join("logs/backend.log");
    let handler: EventHandler = Arc::new(move |frame| {
        let mut r = shared.lock().unwrap();
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
        r.changed(&a);
        drop(r);
        if let Some(reply) = reply {
            if let Some(rpc) = out.lock().unwrap().clone() {
                tauri::async_runtime::spawn(async move {
                    let _ = rpc.write(reply).await;
                });
            }
        }
    });
    let rpc = Rpc::spawn(
        Path::new(&info.path),
        &s.paths.home,
        &cwd,
        generation,
        handler,
    )
    .await?;
    *cell.lock().unwrap() = Some(rpc.clone());
    if let Err(e) = rpc.handshake().await {
        rpc.stop().await;
        return Err(format!("App Server handshake failed: {e}"));
    }
    {
        let mut r = s.shared.lock().unwrap();
        r.rpc = Some(rpc.clone());
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
    if let Ok(account) = rpc
        .request("account/read", json!({"refreshToken":false}))
        .await
    {
        s.shared.lock().unwrap().view.account = account;
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
    changed(s, app);
    Ok(())
}
#[tauri::command]
pub async fn disconnect(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let _op = s.operation.lock().await;
    let rpc = {
        let mut r = s.shared.lock().unwrap();
        r.view.generation += 1;
        r.view.lifecycle = "Stopped".into();
        r.view.turn_id = None;
        r.starting_turn = false;
        r.pending.clear();
        r.view.approvals.clear();
        r.rpc.take()
    };
    if let Some(rpc) = rpc {
        rpc.stop().await;
    }
    changed(&s, &app);
    Ok(())
}
async fn set_project(s: &AppState, path: String) -> Result<(), String> {
    let canonical = canonical_project(&path)?;
    let was_connected = {
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
        r.view.lifecycle = if r.rpc.is_some() { "Ready" } else { "Stopped" }.into();
        r.rpc.is_some()
    };
    save(s)?;
    if was_connected {
        let _ = refresh_threads_inner(s, false).await;
    }
    Ok(())
}
#[tauri::command]
pub async fn select_project(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    if let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("Choose a project Codex may access")
        .pick_folder()
        .await
    {
        let _op = s.operation.lock().await;
        set_project(&s, folder.path().to_string_lossy().into()).await?;
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
async fn refresh_threads_inner(s: &AppState, more: bool) -> Result<(), String> {
    let p = project(s)?;
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
    rpc(&s)?
        .request("thread/archive", json!({"threadId":id}))
        .await?;
    {
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
#[tauri::command]
pub async fn interrupt(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
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
    let (rpc, a) = {
        let r = s.shared.lock().unwrap();
        (
            r.rpc()?,
            r.pending
                .get(&key)
                .cloned()
                .ok_or("This request is no longer pending")?,
        )
    };
    let response = protocol::approval_response(&a.method, &decision, &a.params)?;
    rpc.write(json!({"id":a.id,"result":response})).await?;
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
#[tauri::command]
pub async fn save_preferences(
    app: AppHandle,
    s: State<'_, AppState>,
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
) -> Result<(), String> {
    let _op = s.operation.lock().await;
    {
        let mut r = s.shared.lock().unwrap();
        idle(&r)?;
        if !["dark", "light", "system"].contains(&theme.as_str())
            || !(11..=20).contains(&font_size)
            || !["read-only", "workspace-write", "danger-full-access"].contains(&sandbox.as_str())
            || !["on-request", "never"].contains(&approval_policy.as_str())
            || !["auto", "concise", "detailed", "none"].contains(&reasoning_summary.as_str())
            || !["bottom", "left", "right"].contains(&layout.as_str())
            || !(180..=420).contains(&sidebar_width)
            || !(220..=460).contains(&inspector_width)
        {
            return Err("Unsupported preference value".into());
        }
        if let Some(model) = &model {
            if !r
                .view
                .models
                .iter()
                .any(|m| m["id"] == model.as_str() || m["model"] == model.as_str())
            {
                return Err("Choose a model advertised by this Codex connection".into());
            }
        }
        if let Some(effort) = &reasoning_effort {
            if let Some(selected) = model.as_ref().or(r.view.config.model.as_ref()) {
                if let Some(entry) = r.view.models.iter().find(|m| {
                    m["id"].as_str() == Some(selected) || m["model"].as_str() == Some(selected)
                }) {
                    let advertised = entry["supportedReasoningEfforts"]
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .any(|v| v["reasoningEffort"].as_str() == Some(effort))
                        })
                        .unwrap_or(true);
                    if !advertised {
                        return Err("Choose a reasoning effort advertised by this model".into());
                    }
                }
            }
        }
        if let Some(tier) = &service_tier {
            if let Some(selected) = model.as_ref().or(r.view.config.model.as_ref()) {
                if let Some(entry) = r.view.models.iter().find(|m| {
                    m["id"].as_str() == Some(selected) || m["model"].as_str() == Some(selected)
                }) {
                    let advertised = entry["serviceTiers"]
                        .as_array()
                        .map(|values| values.iter().any(|v| v["id"].as_str() == Some(tier)))
                        .unwrap_or(true);
                    if !advertised {
                        return Err("Choose a service tier advertised by this model".into());
                    }
                }
            }
        }
        let c = &mut r.view.config;
        c.theme = theme;
        c.font_size = font_size;
        c.model = model;
        c.sandbox = sandbox;
        c.approval_policy = approval_policy;
        c.reasoning_effort = reasoning_effort;
        c.reasoning_summary = reasoning_summary;
        c.service_tier = service_tier;
        c.layout = layout;
        c.sidebar_width = sidebar_width;
        c.inspector_width = inspector_width;
        c.debug_log = debug_log;
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
#[tauri::command]
pub async fn choose_binary(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    {
        idle(&s.shared.lock().unwrap())?;
    }
    if let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Select official Codex 0.163.0 x64 executable")
        .add_filter("Windows executable", &["exe"])
        .pick_file()
        .await
    {
        let info = validate_binary(file.path().to_path_buf()).await?;
        let _op = s.operation.lock().await;
        {
            let mut r = s.shared.lock().unwrap();
            idle(&r)?;
            r.view.config.binary_path = Some(info.path.clone());
            r.view.binary = Some(info);
        }
        save(&s)?;
        changed(&s, &app);
    }
    Ok(())
}
#[tauri::command]
pub async fn auth(app: AppHandle, s: State<'_, AppState>, action: String) -> Result<(), String> {
    let _op = s.operation.lock().await;
    let rpc = rpc(&s)?;
    let value = match action.as_str() {
        "status" => {
            let account = rpc
                .request("account/read", json!({"refreshToken":false}))
                .await?;
            s.shared.lock().unwrap().view.account = account;
            Value::Null
        }
        "device" | "browser" => {
            rpc.request(
                "account/login/start",
                json!({"type":if action=="device"{"chatgptDeviceCode"}else{"chatgpt"}}),
            )
            .await?
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
            s.shared.lock().unwrap().view.account = Value::Null;
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
#[tauri::command]
pub async fn diagnostics(s: State<'_, AppState>) -> Result<Value, String> {
    Ok(diagnostic_value(&s))
}
#[tauri::command]
pub async fn wipe_account(app: AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let confirmed=rfd::AsyncMessageDialog::new().set_title("Wipe portable Codex account and history?").set_description("This stops Codex and permanently removes this app's data/codex-home folder, including sign-in state, Codex configuration, and local conversations. Your projects and harness preferences are kept. Continue?").set_level(rfd::MessageLevel::Warning).set_buttons(rfd::MessageButtons::YesNo).show().await;
    if confirmed != rfd::MessageDialogResult::Yes {
        return Ok(());
    }
    let _op = s.operation.lock().await;
    let rpc = {
        let mut r = s.shared.lock().unwrap();
        r.view.generation += 1;
        r.view.lifecycle = "Stopped".into();
        r.pending.clear();
        r.view.approvals.clear();
        r.view.turn_id = None;
        r.starting_turn = false;
        r.rpc.take()
    };
    if let Some(rpc) = rpc {
        rpc.stop().await;
        if !rpc.has_exited() {
            return Err("Codex has not stopped yet. Try again after it exits.".into());
        }
    }
    let metadata = std::fs::symlink_metadata(&s.paths.home).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink()
        || s.paths
            .home
            .canonicalize()
            .map_err(|e| e.to_string())?
            .parent()
            != Some(
                s.paths
                    .data
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .as_path(),
            )
    {
        return Err("Account folder is a link or lies outside the portable data directory; refusing removal.".into());
    }
    std::fs::remove_dir_all(&s.paths.home).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&s.paths.home).map_err(|e| e.to_string())?;
    std::fs::write(
        s.paths.home.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )
    .map_err(|e| e.to_string())?;
    {
        let mut r = s.shared.lock().unwrap();
        r.view.account = Value::Null;
        r.view.login = Value::Null;
        r.view.items.clear();
        r.view.threads.clear();
        r.view.diff.clear();
        r.view.thread_id = None;
        r.view.config.selected_thread = None;
        r.view.error = None;
        r.view.warning =
            Some("Portable Codex account and history were wiped. Connect to sign in again.".into());
    }
    save(&s)?;
    changed(&s, &app);
    Ok(())
}
fn diagnostic_value(s: &AppState) -> Value {
    let r = s.shared.lock().unwrap();
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
    json!({"appVersion":"0.1.0","codexExpected":protocol::CODEX_VERSION,"binary":r.view.binary,"root":s.paths.root,"codexHome":s.paths.home,"lifecycle":r.view.lifecycle,"webview2":"Evergreen runtime required; browser profile is under data/webview2","sandbox":r.view.config.sandbox,"approvalPolicy":r.view.config.approval_policy,"reasoningEffort":r.view.config.reasoning_effort,"serviceTier":r.view.config.service_tier,"authStorage":"Codex file store in data/codex-home; excluded from diagnostics","shells":{"powershell":powershell,"cmd":cmd,"runtimeRequirement":"The harness launches codex.exe directly; shell availability only affects agent-requested commands."},"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH})
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
