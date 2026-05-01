use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tideline_sdk::framing::{read_frame, write_frame};
use tideline_sdk::rpc::{Id, Message, Notification, Request, Response, RpcError};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError),
    #[error("transport closed")]
    Closed,
    #[error("request timed out")]
    Timeout,
}

pub type IncomingRequest = (Request, oneshot::Sender<Response>);

pub struct JsonRpcTransport {
    next_id: Mutex<i64>,
    pending: Arc<Mutex<HashMap<Id, oneshot::Sender<Response>>>>,
    outbound: mpsc::Sender<Message>,
    requests_rx: Mutex<Option<mpsc::Receiver<IncomingRequest>>>,
    notifications_rx: Mutex<Option<mpsc::Receiver<Notification>>>,
    reader_handle: Mutex<Option<JoinHandle<()>>>,
    writer_handle: Mutex<Option<JoinHandle<()>>>,
}

impl JsonRpcTransport {
    pub fn spawn<R, W>(reader: R, writer: W) -> Arc<Self>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);
        let (req_tx, req_rx) = mpsc::channel::<IncomingRequest>(64);
        let (note_tx, note_rx) = mpsc::channel::<Notification>(256);
        let pending: Arc<Mutex<HashMap<Id, oneshot::Sender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_for_reader = pending.clone();
        let out_tx_for_reader = out_tx.clone();
        let reader_handle = tokio::spawn(async move {
            let mut reader = reader;
            loop {
                let frame = match read_frame(&mut reader).await {
                    Ok(Some(f)) => f,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::debug!(error=%e, "transport read err");
                        break;
                    }
                };
                let msg: Message = match serde_json::from_slice(&frame) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(error=%e, "decode failure");
                        continue;
                    }
                };
                match msg {
                    Message::Request(req) => {
                        let id = req.id.clone();
                        let (tx, rx) = oneshot::channel::<Response>();
                        if req_tx.send((req, tx)).await.is_err() {
                            break;
                        }
                        let out_tx = out_tx_for_reader.clone();
                        tokio::spawn(async move {
                            if let Ok(resp) = rx.await {
                                let _ = out_tx.send(Message::Response(resp)).await;
                            } else {
                                let _ = out_tx.send(Message::Response(Response::err(
                                    id,
                                    RpcError {
                                        code: tideline_sdk::rpc::error_codes::INTERNAL_ERROR,
                                        message: "handler dropped".into(),
                                        data: None,
                                    },
                                ))).await;
                            }
                        });
                    }
                    Message::Response(resp) => {
                        if let Some(tx) = pending_for_reader.lock().await.remove(&resp.id) {
                            let _ = tx.send(resp);
                        }
                    }
                    Message::Notification(n) => {
                        let _ = note_tx.send(n).await;
                    }
                }
            }
        });

        let writer_handle = tokio::spawn(async move {
            let mut writer = writer;
            while let Some(msg) = out_rx.recv().await {
                let bytes = match serde_json::to_vec(&msg) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(error=%e, "encode failure");
                        continue;
                    }
                };
                if let Err(e) = write_frame(&mut writer, &bytes).await {
                    tracing::debug!(error=%e, "transport write err");
                    break;
                }
            }
        });

        Arc::new(Self {
            next_id: Mutex::new(1),
            pending,
            outbound: out_tx,
            requests_rx: Mutex::new(Some(req_rx)),
            notifications_rx: Mutex::new(Some(note_rx)),
            reader_handle: Mutex::new(Some(reader_handle)),
            writer_handle: Mutex::new(Some(writer_handle)),
        })
    }

    pub async fn take_requests(&self) -> mpsc::Receiver<IncomingRequest> {
        self.requests_rx.lock().await.take().expect("requests already taken")
    }

    pub async fn take_notifications(&self) -> mpsc::Receiver<Notification> {
        self.notifications_rx.lock().await.take().expect("notes already taken")
    }

    pub async fn call(&self, method: &str, params: Option<Value>, timeout: Duration)
        -> Result<Value, TransportError>
    {
        let id = {
            let mut n = self.next_id.lock().await;
            let id = *n;
            *n += 1;
            Id::Number(id)
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        let req = Request::new(id.clone(), method, params);
        self.outbound.send(Message::Request(req)).await.map_err(|_| TransportError::Closed)?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.error { return Err(TransportError::Rpc(err)); }
                Ok(resp.result.unwrap_or(Value::Null))
            }
            Ok(Err(_)) => Err(TransportError::Closed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(TransportError::Timeout)
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), TransportError> {
        self.outbound
            .send(Message::Notification(Notification::new(method, params)))
            .await
            .map_err(|_| TransportError::Closed)
    }

    pub async fn shutdown(&self) {
        if let Some(h) = self.writer_handle.lock().await.take() { h.abort(); }
        if let Some(h) = self.reader_handle.lock().await.take() { h.abort(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::duplex;

    #[tokio::test]
    async fn host_request_gets_response() {
        let (host_r, plugin_w) = duplex(8192);
        let (plugin_r, host_w) = duplex(8192);
        let host = JsonRpcTransport::spawn(host_r, host_w);
        let plugin = JsonRpcTransport::spawn(plugin_r, plugin_w);

        let mut plugin_reqs = plugin.take_requests().await;
        tokio::spawn(async move {
            while let Some((req, ack)) = plugin_reqs.recv().await {
                let _ = ack.send(Response::ok(req.id, json!({"echo": req.method})));
            }
        });

        let val = host.call("host/initialize", Some(json!({})), Duration::from_secs(2)).await.unwrap();
        assert_eq!(val, json!({"echo":"host/initialize"}));
    }

    #[tokio::test]
    async fn timeout_returns_error() {
        let (host_r, _plugin_w) = duplex(8192);
        let (_plugin_r, host_w) = duplex(8192);
        let host = JsonRpcTransport::spawn(host_r, host_w);
        let err = host.call("host/anything", None, Duration::from_millis(50)).await.unwrap_err();
        assert!(matches!(err, TransportError::Timeout));
    }
}
