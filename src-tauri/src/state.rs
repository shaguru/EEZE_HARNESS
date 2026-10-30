use crate::{claude, netenv::Notice, portable_paths::Config, protocol, rpc_transport::Rpc};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryInfo {
    pub path: String,
    pub version: String,
    pub sha256: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub key: String,
    pub id: Value,
    pub method: String,
    pub params: Value,
    pub supported: bool,
}
/// A background task the student can watch: a tool download or a new project being set up.
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub title: String,
    /// "running", "done" or "failed".
    pub state: String,
    pub percent: Option<u8>,
    pub message: String,
    pub log: Vec<String>,
    pub result: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub lifecycle: String,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub config: Config,
    pub root: String,
    pub binary: Option<BinaryInfo>,
    pub models: Vec<Value>,
    pub account: Value,
    pub login: Value,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub threads: Vec<Value>,
    pub items: Vec<Value>,
    pub approvals: Vec<Approval>,
    pub diff: String,
    pub capabilities: Vec<String>,
    pub generation: u64,
    pub thread_cursor: Option<String>,
    pub history_cursor: Option<String>,
    /// School-computer problems found at startup (unzipped in temp, OneDrive, low disk, ...).
    pub notices: Vec<Notice>,
    pub claude_binary: Option<BinaryInfo>,
    /// Model, permission mode and shell reported by the running Claude session.
    pub claude_info: Value,
    /// Which agent programs and shells were found beside the app.
    pub engines: Value,
    /// Portable programming tools (Python, Rust, Git, PowerShell, Node.js) and their state.
    pub tools: Value,
    pub templates: Value,
    pub jobs: Vec<Job>,
    /// How the selected project runs ({run, tool, toolReady}), for the Run button.
    pub project_info: Value,
    /// Codex started before tools were added, so it cannot see them until it restarts.
    pub tools_stale: bool,
    pub projects_dir: String,
}
pub struct Runtime {
    pub view: Snapshot,
    pub rpc: Option<Rpc>,
    pub pending: BTreeMap<String, Approval>,
    pub starting_turn: bool,
    pub claude: Option<claude::Session>,
    /// Bumped whenever a Claude process is replaced, so events from an old process are ignored.
    pub claude_gen: u64,
    /// The open Claude conversation already exists on disk, so restarts use --resume.
    pub claude_has_history: bool,
    pub claude_msg: Option<String>,
    pub claude_interrupting: bool,
    pub claude_login: Option<claude::Login>,
    pub login_gen: u64,
    /// Config changed from an event (usage); the event handler saves it.
    pub config_dirty: bool,
    pub seq: u64,
    /// Tools fingerprint when the running Codex started.
    pub agent_tools: String,
}
impl Runtime {
    pub fn new(config: Config, root: String) -> Self {
        Self {
            view: Snapshot {
                lifecycle: "Stopped".into(),
                error: None,
                warning: None,
                config,
                root,
                binary: None,
                models: vec![],
                account: Value::Null,
                login: Value::Null,
                thread_id: None,
                turn_id: None,
                threads: vec![],
                items: vec![],
                approvals: vec![],
                diff: String::new(),
                capabilities: protocol::capabilities(),
                generation: 0,
                thread_cursor: None,
                history_cursor: None,
                notices: vec![],
                claude_binary: None,
                claude_info: Value::Null,
                engines: Value::Null,
                tools: Value::Null,
                templates: Value::Null,
                jobs: vec![],
                project_info: Value::Null,
                tools_stale: false,
                projects_dir: String::new(),
            },
            rpc: None,
            pending: Default::default(),
            starting_turn: false,
            claude: None,
            claude_gen: 0,
            claude_has_history: false,
            claude_msg: None,
            claude_interrupting: false,
            claude_login: None,
            login_gen: 0,
            config_dirty: false,
            seq: 0,
            agent_tools: String::new(),
        }
    }
    /// Creates or updates a job; at most the last 40 log lines are kept.
    pub fn job(&mut self, id: &str, update: impl FnOnce(&mut Job)) {
        let index = match self.view.jobs.iter().position(|j| j.id == id) {
            Some(i) => i,
            None => {
                self.view.jobs.push(Job { id: id.into(), state: "running".into(), ..Default::default() });
                self.view.jobs.len() - 1
            }
        };
        let job = &mut self.view.jobs[index];
        update(job);
        if job.log.len() > 40 {
            let extra = job.log.len() - 40;
            job.log.drain(..extra);
        }
    }
    pub fn next_id(&mut self, prefix: &str) -> String {
        self.seq += 1;
        format!("{prefix}-{}", self.seq)
    }
    /// Stores usage for the active account of a provider. With merge, windows missing from a sparse
    /// update keep their previous value.
    pub fn set_usage(&mut self, provider: &str, usage: Value, merge: bool) {
        let id = self.view.config.active_account(provider).id;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(account) = self.view.config.account_mut(&id) {
            let mut next = if merge {
                account.usage.clone().unwrap_or_else(|| json!({}))
            } else {
                json!({})
            };
            if let (Some(target), Some(src)) = (next.as_object_mut(), usage.as_object()) {
                for (k, v) in src {
                    if !v.is_null() {
                        target.insert(k.clone(), v.clone());
                    }
                }
            }
            if account.usage.as_ref() != Some(&next) {
                account.usage = Some(next);
                account.usage_at = Some(now);
                self.config_dirty = true;
            }
        }
    }
    pub fn trim_items(&mut self) {
        if self.view.items.len() > 1600
            || self
                .view
                .items
                .iter()
                .map(|i| i.to_string().len())
                .sum::<usize>()
                > 12 * 1024 * 1024
        {
            let count = (self.view.items.len() / 4).max(1);
            self.view.items.drain(..count);
            self.view.warning = Some(
                "Older visible items were trimmed from memory. Reopen the thread to load history."
                    .into(),
            );
        }
    }
    pub fn active(&self) -> bool {
        self.starting_turn || self.view.turn_id.is_some() || !self.pending.is_empty()
    }
    pub fn rpc(&self) -> Result<Rpc, String> {
        if !["Ready", "Busy", "Waiting for Approval"].contains(&self.view.lifecycle.as_str()) {
            return Err("Connect or restart Codex first.".into());
        }
        self.rpc.clone().ok_or("Codex is disconnected".into())
    }
    pub fn changed(&self, app: &AppHandle) {
        let _ = app.emit("harness-changed", self.view.generation);
    }
    pub fn fail(&mut self, text: String) {
        self.view.lifecycle = "Failed".into();
        self.view.error = Some(text);
        self.view.turn_id = None;
        self.pending.clear();
        self.view.approvals.clear();
        self.starting_turn = false;
    }
    fn upsert(&mut self, item: Value) {
        let id = item["id"].as_str().unwrap_or("");
        if id.is_empty() {
            return;
        }
        if let Some(i) = self.view.items.iter_mut().find(|i| i["id"] == id) {
            *i = item;
        } else {
            self.view.items.push(item);
        }
        self.trim_items();
    }
    pub fn handle(&mut self, frame: Value) -> Option<Value> {
        if let Some(e) = frame.get("localError").and_then(Value::as_str) {
            self.fail(e.into());
            return None;
        }
        if frame.get("localExit").is_some() {
            self.fail("Codex stopped. Restart Codex, then resume the thread; the client never replays a command.".into());
            return None;
        }
        let method = frame["method"].as_str().unwrap_or("");
        let p = &frame["params"];
        if let Some(id) = frame.get("id") {
            if !id.is_string() && !id.is_number() {
                return None;
            }
            if self.pending.len() >= 32 {
                return Some(
                    json!({"id":id,"error":{"code":-32603,"message":"Too many pending client requests; request refused"}}),
                );
            }
            if matches!(
                method,
                "account/chatgptAuthTokens/refresh" | "attestation/generate" | "item/tool/call"
            ) {
                return Some(
                    json!({"id":id,"error":{"code":-32601,"message":"Client capability not provided"}}),
                );
            }
            if self.view.config.approval_policy == "never" && protocol::supports_approval(method) {
                if let Ok(response) = protocol::approval_response(method, "accept", p) {
                    return Some(json!({"id":id,"result":response}));
                }
            }
            let key = id.to_string();
            let a = Approval {
                key: key.clone(),
                id: id.clone(),
                method: method.into(),
                params: p.clone(),
                supported: protocol::supports_approval(method),
            };
            self.pending.insert(key, a);
            self.view.approvals = self.pending.values().cloned().collect();
            self.view.lifecycle = "Waiting for Approval".into();
            return None;
        }
        if let Some(thread) = p.get("threadId").and_then(Value::as_str) {
            if self.view.thread_id.as_deref() != Some(thread) {
                return None;
            }
        }
        match method {
            "turn/started" => {
                self.starting_turn = false;
                self.view.turn_id = p["turn"]["id"].as_str().map(str::to_string);
                self.view.lifecycle = "Busy".into();
                self.view.diff.clear();
            }
            "turn/completed" => {
                self.starting_turn = false;
                self.view.turn_id = None;
                self.pending.clear();
                self.view.approvals.clear();
                self.view.lifecycle = "Ready".into();
                if let Some(e) = p["turn"]["error"]["message"].as_str() {
                    self.view.error = Some(e.into());
                }
            }
            "item/started" | "item/completed" => {
                let mut item = p["item"].clone();
                if method == "item/completed" {
                    item["harnessCompleted"] = json!(true);
                }
                self.upsert(item);
            }
            "item/agentMessage/delta" => {
                let id = p["itemId"].as_str().unwrap_or("");
                let delta = p["delta"].as_str().unwrap_or("");
                if !self.view.items.iter().any(|i| i["id"] == id) {
                    self.upsert(json!({"id":id,"type":"agentMessage","text":""}));
                }
                if let Some(item) = self.view.items.iter_mut().find(|i| i["id"] == id) {
                    append(item, "text", delta, 2 * 1024 * 1024);
                }
            }
            "item/commandExecution/outputDelta" => {
                let id = p["itemId"].as_str().unwrap_or("");
                if let Some(item) = self.view.items.iter_mut().find(|i| i["id"] == id) {
                    append(
                        item,
                        "aggregatedOutput",
                        p["delta"].as_str().unwrap_or(""),
                        512 * 1024,
                    );
                }
            }
            "item/fileChange/patchUpdated" => {
                let id = p["itemId"].as_str().unwrap_or("");
                if let Some(item) = self.view.items.iter_mut().find(|i| i["id"] == id) {
                    item["changes"] = p["changes"].clone();
                } else {
                    self.upsert(json!({"type":"fileChange","id":id,"changes":p["changes"]}));
                }
            }
            "turn/diff/updated" => {
                self.view.diff = p["diff"].as_str().unwrap_or("").to_string();
            }
            "error" => {
                self.view.error = p["error"]["message"].as_str().map(str::to_string);
            }
            "account/login/completed" => {
                self.view.login = Value::Null;
                if p["success"] != true {
                    self.view.error = Some(
                        p["error"]
                            .as_str()
                            .unwrap_or("Sign-in failed; check browser and network policy.")
                            .into(),
                    );
                } else {
                    self.view.warning = Some("Signed in. You can start chatting.".into());
                }
            }
            "account/updated" => {}
            "account/rateLimits/updated" => {
                let usage = codex_usage(&p["rateLimits"]);
                self.set_usage("codex", usage, true);
            }
            "warning" => {
                self.view.warning = p["message"].as_str().map(str::to_string);
            }
            "windowsSandbox/setupCompleted" => {
                self.view.warning=Some(if p["success"]==true{"Windows sandbox setup completed."}else{"Windows sandbox setup failed. See Codex's supported setup and school policy."}.into());
            }
            _ => {}
        }
        None
    }
}
/// Codex reports a primary (usually 5-hour) and secondary (usually weekly) window.
pub fn codex_usage(s: &Value) -> Value {
    let window = |w: &Value| {
        if w.is_object() {
            json!({"usedPercent": w["usedPercent"], "windowMins": w["windowDurationMins"], "resetsAt": w["resetsAt"]})
        } else {
            Value::Null
        }
    };
    json!({
        "primary": window(&s["primary"]),
        "secondary": window(&s["secondary"]),
        "limitReached": s["rateLimitReachedType"].is_string(),
        "plan": s["planType"],
    })
}
pub(crate) fn append(item: &mut Value, field: &str, delta: &str, limit: usize) {
    let text = item[field].as_str().unwrap_or("");
    if text.len() + delta.len() <= limit {
        item[field] = json!(format!("{text}{delta}"));
    } else if !text.ends_with("[Output truncated]") {
        item[field] = json!(format!("{text}\n[Output truncated]"));
    }
}
pub type Shared = std::sync::Arc<Mutex<Runtime>>;

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> Runtime {
        let mut c = Config::default();
        c.normalize();
        let mut r = Runtime::new(c, "test".into());
        r.view.thread_id = Some("t".into());
        r
    }
    #[test]
    fn remount_snapshot_and_out_of_order_delta() {
        let mut r = runtime();
        r.handle(json!({"method":"item/agentMessage/delta","params":{"threadId":"t","itemId":"a","delta":"hello"}}));
        assert_eq!(r.view.items[0]["text"], "hello");
        r.handle(json!({"method":"item/completed","params":{"threadId":"t","item":{"id":"a","type":"agentMessage","text":"hello world"}}}));
        assert_eq!(r.view.items.len(), 1);
        assert_eq!(r.view.items[0]["text"], "hello world");
    }
    #[test]
    fn approvals_never_auto_accept_and_clear_on_exit() {
        let mut r = runtime();
        assert!(r.handle(json!({"id":5,"method":"item/commandExecution/requestApproval","params":{"command":"whoami"}})).is_none());
        assert_eq!(r.pending.len(), 1);
        r.handle(json!({"localExit":1}));
        assert!(r.pending.is_empty());
        assert_eq!(r.view.lifecycle, "Failed");
    }
    #[test]
    fn never_policy_auto_accepts_supported_requests() {
        let mut r = runtime();
        r.view.config.approval_policy = "never".into();
        let reply = r.handle(json!({"id":5,"method":"item/commandExecution/requestApproval","params":{"command":"echo allowed"}}));
        assert_eq!(reply.unwrap()["result"]["decision"], "accept");
        assert!(r.pending.is_empty());
    }
    #[test]
    fn codex_rate_limit_updates_merge_into_usage() {
        let mut r = runtime();
        r.handle(json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"primary":{"usedPercent":30,"windowDurationMins":300,"resetsAt":5},"secondary":null}}}));
        let usage = r.view.config.active_account("codex").usage.unwrap();
        assert_eq!(usage["primary"]["usedPercent"], 30);
        assert!(r.config_dirty);
    }
    #[test]
    fn active_turn_and_thread_filter() {
        let mut r = runtime();
        r.handle(json!({"method":"turn/started","params":{"threadId":"other","turn":{"id":"x"}}}));
        assert!(!r.active());
        r.handle(json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"x"}}}));
        assert!(r.active());
        r.handle(json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"x","status":"interrupted"}}}));
        assert!(!r.active());
        assert_eq!(r.view.lifecycle, "Ready");
    }
}
