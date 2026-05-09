use std::sync::Arc;
use std::time::Duration;
use serde_json::Value;
use tokio::io::stdin;
use tokio::sync::{Mutex, mpsc, oneshot};
use crate::framing::read_frame;
use crate::rpc::{Id, Message, Notification, Request, Response, RpcError, error_codes};

pub type IncomingRequest = (Request, oneshot::Sender<Response>);

#[derive(thiserror::Error, Debug)]
pub enum SdkTransportError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("rpc: {0}")] Rpc(#[from] RpcError),
    #[error("closed")] Closed,
    #[error("timeout")] Timeout,
    #[error("decode: {0}")] Decode(String),
}

pub struct StdioTransport {
    next_id: Mutex<i64>,
    pending: Arc<Mutex<std::collections::HashMap<Id, oneshot::Sender<Response>>>>,
    outbound: mpsc::Sender<Message>,
    requests_rx: Mutex<Option<mpsc::Receiver<IncomingRequest>>>,
    notifications_rx: Mutex<Option<mpsc::Receiver<Notification>>>,
}

impl StdioTransport {
    pub fn spawn() -> Arc<Self> {
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);
        let (req_tx, req_rx) = mpsc::channel::<IncomingRequest>(64);
        let (note_tx, note_rx) = mpsc::channel::<Notification>(256);
        let pending: Arc<Mutex<std::collections::HashMap<Id, oneshot::Sender<Response>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Redirect rogue writes to fd 1 (stdout) so libc printf/fprintf from
        // native deps (livi/lilv/jack/Cairo/X11/the LV2 plugin's UI thread)
        // can't corrupt our JSON-RPC framing on the host pipe. We dup fd 1
        // first so we keep a private handle for IPC, then point fd 1 at
        // /dev/null. Anything that bypasses Rust and writes to fd 1 directly
        // now goes to /dev/null instead of the parent's frame parser.
        let ipc_writer_fd = unsafe {
            let saved = libc::dup(libc::STDOUT_FILENO);
            if saved < 0 {
                panic!(
                    "tideline-sdk: dup(stdout) failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            let devnull = libc::open(b"/dev/null\0".as_ptr() as *const _, libc::O_WRONLY);
            if devnull < 0 {
                panic!(
                    "tideline-sdk: open(/dev/null) failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            if libc::dup2(devnull, libc::STDOUT_FILENO) < 0 {
                panic!(
                    "tideline-sdk: dup2(devnull,stdout) failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            libc::close(devnull);
            saved
        };

        let pending_clone = pending.clone();
        let out_tx_for_reader = out_tx.clone();
        tokio::spawn(async move {
            let mut stdin = stdin();
            loop {
                let frame = match read_frame(&mut stdin).await {
                    Ok(Some(f)) => f,
                    _ => break,
                };
                let msg: Message = match serde_json::from_slice(&frame) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                match msg {
                    Message::Request(req) => {
                        let id = req.id.clone();
                        let (tx, rx) = oneshot::channel::<Response>();
                        let _ = req_tx.send((req, tx)).await;
                        let out = out_tx_for_reader.clone();
                        tokio::spawn(async move {
                            if let Ok(resp) = rx.await {
                                let _ = out.send(Message::Response(resp)).await;
                            } else {
                                let _ = out.send(Message::Response(Response::err(id, RpcError {
                                    code: error_codes::INTERNAL_ERROR,
                                    message: "handler dropped".into(),
                                    data: None,
                                }))).await;
                            }
                        });
                    }
                    Message::Response(resp) => {
                        if let Some(tx) = pending_clone.lock().await.remove(&resp.id) {
                            let _ = tx.send(resp);
                        }
                    }
                    Message::Notification(n) => { let _ = note_tx.send(n).await; }
                }
            }
        });

        // Sync writer thread on the saved IPC fd. Uses blocking_recv so it
        // doesn't tie up a tokio worker; pipe writes to a parent that's
        // actively reading rarely block long enough to matter. spawn_blocking
        // routes us to tokio's blocking pool which is sized for I/O like this.
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            use std::os::fd::FromRawFd;
            // SAFETY: ipc_writer_fd is a freshly duped fd we own exclusively.
            let mut writer = unsafe { std::fs::File::from_raw_fd(ipc_writer_fd) };
            while let Some(msg) = out_rx.blocking_recv() {
                let bytes = match serde_json::to_vec(&msg) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
                if writer.write_all(header.as_bytes()).is_err() { break; }
                if writer.write_all(&bytes).is_err() { break; }
                if writer.flush().is_err() { break; }
            }
        });

        Arc::new(Self {
            next_id: Mutex::new(1),
            pending,
            outbound: out_tx,
            requests_rx: Mutex::new(Some(req_rx)),
            notifications_rx: Mutex::new(Some(note_rx)),
        })
    }

    pub async fn take_requests(&self) -> mpsc::Receiver<IncomingRequest> {
        self.requests_rx.lock().await.take().expect("already taken")
    }

    pub async fn take_notifications(&self) -> mpsc::Receiver<Notification> {
        self.notifications_rx.lock().await.take().expect("already taken")
    }

    pub async fn call(&self, method: &str, params: Option<Value>, timeout: Duration)
        -> Result<Value, SdkTransportError>
    {
        let id = {
            let mut n = self.next_id.lock().await;
            let id = *n; *n += 1; Id::Number(id)
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        let req = Request::new(id.clone(), method, params);
        self.outbound.send(Message::Request(req)).await.map_err(|_| SdkTransportError::Closed)?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.error { return Err(SdkTransportError::Rpc(err)); }
                Ok(resp.result.unwrap_or(Value::Null))
            }
            Ok(Err(_)) => Err(SdkTransportError::Closed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(SdkTransportError::Timeout)
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), SdkTransportError> {
        self.outbound
            .send(Message::Notification(Notification::new(method, params)))
            .await
            .map_err(|_| SdkTransportError::Closed)
    }
}
