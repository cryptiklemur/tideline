use crate::client::HostClient;
use crate::rpc::{error_codes, Notification, Request, Response, RpcError};
use crate::transport::StdioTransport;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    async fn on_ready(&self, _host: Arc<HostClient>) {}
    async fn on_event(&self, _host: Arc<HostClient>, _topic: String, _params: Value) {}
    async fn on_notification(
        &self,
        _host: Arc<HostClient>,
        _method: String,
        _params: Option<Value>,
    ) {
    }
    async fn on_request(
        &self,
        _host: Arc<HostClient>,
        method: String,
        _params: Option<Value>,
    ) -> Result<Value, RpcError> {
        Err(RpcError {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("plugin handler missing for {method}"),
            data: None,
        })
    }
}

pub async fn run<P: Plugin>(plugin: P) {
    let transport = StdioTransport::spawn();
    let host = Arc::new(HostClient::new(transport.clone()));
    let plugin = Arc::new(plugin);

    let mut requests = transport.take_requests().await;
    let mut notifications = transport.take_notifications().await;

    let p = plugin.clone();
    let h = host.clone();
    tokio::spawn(async move {
        p.on_ready(h).await;
    });

    let p_req = plugin.clone();
    let h_req = host.clone();
    let req_task = tokio::spawn(async move {
        while let Some((
            Request {
                id, method, params, ..
            },
            ack,
        )) = requests.recv().await
        {
            let h = h_req.clone();
            let p = p_req.clone();
            tokio::spawn(async move {
                let resp = match p.on_request(h, method, params).await {
                    Ok(v) => Response::ok(id, v),
                    Err(e) => Response::err(id, e),
                };
                let _ = ack.send(resp);
            });
        }
    });

    let p_evt = plugin.clone();
    let h_evt = host.clone();
    let evt_task = tokio::spawn(async move {
        while let Some(Notification { method, params, .. }) = notifications.recv().await {
            match method.as_str() {
                "host/event.fire" => {
                    if let Some(obj) = params.as_ref().and_then(|v| v.as_object()) {
                        let topic = obj
                            .get("topic")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let evt_params = obj.get("params").cloned().unwrap_or(Value::Null);
                        p_evt.on_event(h_evt.clone(), topic, evt_params).await;
                    }
                }
                _ => {
                    p_evt.on_notification(h_evt.clone(), method, params).await;
                }
            }
        }
    });

    let _ = tokio::join!(req_task, evt_task);
}
