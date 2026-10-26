use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{FramedRead, LinesCodec};

pub const MAX_FRAME: usize = 8 * 1024 * 1024;
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;
pub type EventHandler = Arc<dyn Fn(Value) + Send + Sync>;

#[derive(Clone)]
pub struct Rpc {
    tx: mpsc::Sender<Value>,
    pending: Pending,
    next: Arc<AtomicU64>,
    stop: mpsc::Sender<()>,
    exited: Arc<AtomicBool>,
}
struct RequestGuard {
    id: u64,
    pending: Pending,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.id);
    }
}
impl Rpc {
    pub async fn spawn(
        binary: &Path,
        home: &Path,
        cwd: &Path,
        _generation: u64,
        handler: EventHandler,
    ) -> Result<Self, String> {
        let mut command = Command::new(binary);
        command
            .args([
                "app-server",
                "--listen",
                "stdio://",
                "-c",
                "cli_auth_credentials_store=\"file\"",
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "sandbox_mode=\"read-only\"",
            ])
            .env("CODEX_HOME", home)
            .env(
                "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
                "rust_portable_harness",
            )
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(root) = binary.parent() {
            let tools = root.join("codex-path");
            if tools.is_dir() {
                let mut paths = vec![tools];
                paths.extend(std::env::split_paths(
                    &std::env::var_os("PATH").unwrap_or_default(),
                ));
                if let Ok(path) = std::env::join_paths(paths) {
                    command.env("PATH", path);
                }
            }
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start companion Codex executable: {e}"))?;
        #[cfg(windows)]
        let job = Job::assign(&child)?;
        let mut stdin = child.stdin.take().ok_or("Codex stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("Codex stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("Codex stderr unavailable")?;
        let (tx, mut rx) = mpsc::channel::<Value>(128);
        let (stop, mut stoprx) = mpsc::channel::<()>(1);
        let pending: Pending = Default::default();
        let exited = Arc::new(AtomicBool::new(false));
        let did_exit = exited.clone();
        let rpc = Self {
            tx,
            pending: pending.clone(),
            next: Arc::new(AtomicU64::new(1)),
            stop,
            exited,
        };
        let fail = handler.clone();
        let kill_on_write = rpc.stop.clone();
        tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                let mut bytes = match serde_json::to_vec(&value) {
                    Ok(b) => b,
                    Err(_) => break,
                };
                bytes.push(b'\n');
                if stdin.write_all(&bytes).await.is_err() {
                    fail(json!({"localError":"Codex input pipe closed"}));
                    let _ = kill_on_write.send(()).await;
                    break;
                }
            }
        });
        let p = pending.clone();
        let h = handler.clone();
        let kill_on_frame = rpc.stop.clone();
        tokio::spawn(async move {
            let mut lines = FramedRead::new(stdout, LinesCodec::new_with_max_length(MAX_FRAME));
            while let Some(line) = lines.next().await {
                match line {
                    Ok(line) => match parse_frame(&line) {
                        Ok(v) => dispatch(v, &p, &h),
                        Err(e) => {
                            h(json!({"localError":e}));
                            let _ = kill_on_frame.send(()).await;
                            break;
                        }
                    },
                    Err(_) => {
                        h(
                            json!({"localError":"Invalid UTF-8 or oversized Codex protocol frame (limit 8 MiB). Restart Codex."}),
                        );
                        let _ = kill_on_frame.send(()).await;
                        break;
                    }
                }
            }
        });
        // Never forward stderr: authentication URLs, credentials, and project contents may appear there.
        tokio::spawn(async move {
            let mut lines = FramedRead::new(stderr, LinesCodec::new_with_max_length(65536));
            while let Some(_line) = lines.next().await {}
        });
        tokio::spawn(async move {
            let result = tokio::select! { result=child.wait()=>result, _=stoprx.recv()=>{let _=child.kill().await; child.wait().await} };
            #[cfg(windows)]
            drop(job);
            fail_pending(&pending,"Codex exited; pending operations were cancelled. Restart Codex and resume the thread.");
            handler(json!({"localExit":result.ok().and_then(|s|s.code())}));
            did_exit.store(true, Ordering::Release);
        });
        Ok(rpc)
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let _guard = RequestGuard {
            id,
            pending: self.pending.clone(),
        };
        let frame = json!({"id":id,"method":method,"params":params});
        if self.tx.send(frame).await.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err("Codex is disconnected".into());
        }
        match tokio::time::timeout(Duration::from_secs(45), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Codex response cancelled".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("Codex timed out waiting for {method}. Check network/authentication, then reconnect."))
            }
        }
    }
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write(json!({"method":method,"params":params})).await
    }
    pub async fn write(&self, value: Value) -> Result<(), String> {
        self.tx
            .send(value)
            .await
            .map_err(|_| "Codex is disconnected".into())
    }
    pub async fn stop(&self) {
        let _ = self.stop.send(()).await;
        for _ in 0..100 {
            if self.exited.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub fn stop_now(&self) {
        let _ = self.stop.try_send(());
    }
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }
    pub async fn handshake(&self) -> Result<Value, String> {
        let result=self.request("initialize",json!({"clientInfo":{"name":"rust_portable_harness","title":"RustPortableHarness","version":"0.1.0"},"capabilities":{"experimentalApi":false}})).await?;
        self.notify("initialized", json!({})).await?;
        Ok(result)
    }
}
pub fn parse_frame(line: &str) -> Result<Value, String> {
    if line.len() > MAX_FRAME {
        return Err("Oversized protocol frame".into());
    }
    let value: Value = serde_json::from_str(line)
        .map_err(|_| "Malformed JSON from Codex; protocol connection stopped".to_string())?;
    if !value.is_object() {
        return Err("Codex frame must be an object".into());
    }
    Ok(value)
}
fn dispatch(v: Value, p: &Pending, h: &EventHandler) {
    if v.get("method").is_some() {
        h(v);
    } else if let Some(id) = v.get("id").and_then(Value::as_u64) {
        if let Some(tx) = p.lock().unwrap().remove(&id) {
            let result = if let Some(e) = v.get("error") {
                Err(format!(
                    "Codex RPC error {}: {}",
                    e["code"],
                    e["message"].as_str().unwrap_or("Unknown error")
                ))
            } else if let Some(r) = v.get("result") {
                Ok(r.clone())
            } else {
                Err("Codex response has no result or error".into())
            };
            let _ = tx.send(result);
        }
    }
}
fn fail_pending(p: &Pending, message: &str) {
    for (_, tx) in p.lock().unwrap().drain() {
        let _ = tx.send(Err(message.into()));
    }
}

#[cfg(windows)]
struct Job(isize);
#[cfg(windows)]
impl Job {
    fn assign(child: &tokio::process::Child) -> Result<Self, String> {
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("Windows could not contain Codex child processes".into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(
                    handle,
                    child.raw_handle().ok_or("Codex process handle missing")? as _,
                ) == 0
            {
                CloseHandle(handle);
                return Err(format!(
                    "Windows could not contain Codex child processes: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self(handle as isize))
        }
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_unicode_malformed_size() {
        assert_eq!(
            parse_frame(r#"{"result":"สวัสดี"}"#).unwrap()["result"],
            "สวัสดี"
        );
        assert!(parse_frame("[]").is_err());
        assert!(parse_frame("bad").is_err());
        assert!(parse_frame(&" ".repeat(MAX_FRAME + 1)).is_err());
    }
    #[tokio::test]
    async fn fragmented_and_multiple_lines() {
        use tokio::io::AsyncWriteExt;
        let (mut w, r) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            w.write_all(b"{\"a\":").await.unwrap();
            w.write_all("\"字\"}\n{\"b\":2}\n".as_bytes())
                .await
                .unwrap();
        });
        let mut lines = FramedRead::new(r, LinesCodec::new_with_max_length(MAX_FRAME));
        assert_eq!(
            parse_frame(&lines.next().await.unwrap().unwrap()).unwrap()["a"],
            "字"
        );
        assert_eq!(
            parse_frame(&lines.next().await.unwrap().unwrap()).unwrap()["b"],
            2
        );
    }
    #[tokio::test]
    async fn out_of_order_duplicate_and_server_requests() {
        let p: Pending = Default::default();
        let (tx1, rx1) = oneshot::channel();
        let (tx2, rx2) = oneshot::channel();
        p.lock().unwrap().insert(1, tx1);
        p.lock().unwrap().insert(2, tx2);
        let count = Arc::new(AtomicU64::new(0));
        let c = count.clone();
        let h: EventHandler = Arc::new(move |_| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        dispatch(json!({"id":2,"result":"two"}), &p, &h);
        dispatch(json!({"id":2,"result":"duplicate"}), &p, &h);
        dispatch(json!({"id":1,"method":"approval"}), &p, &h);
        dispatch(json!({"id":1,"result":"one"}), &p, &h);
        assert_eq!(rx1.await.unwrap().unwrap(), "one");
        assert_eq!(rx2.await.unwrap().unwrap(), "two");
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert!(p.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn exit_clears_pending() {
        let p: Pending = Default::default();
        let (tx, rx) = oneshot::channel();
        p.lock().unwrap().insert(1, tx);
        fail_pending(&p, "exited");
        assert!(rx.await.unwrap().is_err());
        assert!(p.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn initialize_response_precedes_initialized() {
        let (tx, mut rx) = mpsc::channel(10);
        let (stop, _) = mpsc::channel(1);
        let p: Pending = Default::default();
        let rpc = Rpc {
            tx,
            pending: p.clone(),
            next: Arc::new(AtomicU64::new(1)),
            stop,
            exited: Arc::new(AtomicBool::new(false)),
        };
        let task = tokio::spawn(async move { rpc.handshake().await });
        let first = rx.recv().await.unwrap();
        assert_eq!(first["method"], "initialize");
        assert!(rx.try_recv().is_err());
        let h: EventHandler = Arc::new(|_| {});
        dispatch(
            json!({"id":first["id"],"result":{"userAgent":"test"}}),
            &p,
            &h,
        );
        assert_eq!(rx.recv().await.unwrap()["method"], "initialized");
        assert!(task.await.unwrap().is_ok());
    }
    #[tokio::test]
    async fn cancelled_request_is_removed() {
        let (tx, mut rx) = mpsc::channel(10);
        let (stop, _) = mpsc::channel(1);
        let p: Pending = Default::default();
        let rpc = Rpc {
            tx,
            pending: p.clone(),
            next: Arc::new(AtomicU64::new(1)),
            stop,
            exited: Arc::new(AtomicBool::new(false)),
        };
        let task = tokio::spawn(async move { rpc.request("model/list", json!({})).await });
        let _ = rx.recv().await;
        assert_eq!(p.lock().unwrap().len(), 1);
        task.abort();
        let _ = task.await;
        assert!(p.lock().unwrap().is_empty());
    }
    #[tokio::test]
    #[ignore = "uses the verified companion executable from the workspace"]
    async fn real_companion_handshake() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let handler: EventHandler = Arc::new(|_| {});
        let rpc = Rpc::spawn(&root.join("codex.exe"), temp.path(), root, 0, handler)
            .await
            .unwrap();
        let initialized = rpc.handshake().await.unwrap();
        assert!(initialized["userAgent"].is_string());
        let models = rpc
            .request("model/list", json!({"limit":100}))
            .await
            .unwrap();
        assert!(!models["data"].as_array().unwrap().is_empty());
        rpc.stop().await;
        assert!(rpc.exited.load(Ordering::Acquire));
    }
}
