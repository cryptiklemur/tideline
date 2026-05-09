use crate::capabilities::CapabilitySet;
use crate::logging::PluginLog;
use crate::transport::JsonRpcTransport;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(Debug, Clone)]
pub enum PluginState {
    Stopped,
    Spawning,
    Running,
    Errored(String),
    ShuttingDown,
}

pub struct PluginRuntime {
    pub plugin_id: String,
    pub install_dir: PathBuf,
    pub exec: String,
    pub capabilities: Arc<RwLock<CapabilitySet>>,
    pub state: Arc<RwLock<PluginState>>,
    pub log: Arc<PluginLog>,
    transport: Arc<Mutex<Option<Arc<JsonRpcTransport>>>>,
    child: Arc<Mutex<Option<Child>>>,
    pub exit_tx: mpsc::Sender<i32>,
}

impl PluginRuntime {
    pub async fn spawn(
        plugin_id: String,
        install_dir: PathBuf,
        exec: String,
        capabilities: Arc<RwLock<CapabilitySet>>,
        log: Arc<PluginLog>,
        exit_tx: mpsc::Sender<i32>,
    ) -> std::io::Result<Arc<Self>> {
        let state = Arc::new(RwLock::new(PluginState::Spawning));
        let exec_path = install_dir.join(&exec);
        let mut cmd = Command::new(&exec_path);
        cmd.current_dir(&install_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        let log_for_stderr = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = log_for_stderr.append_line("STDERR", &line).await;
            }
        });

        let transport = JsonRpcTransport::spawn(stdout, stdin);
        *state.write().await = PluginState::Running;

        let runtime = Arc::new(Self {
            plugin_id,
            install_dir,
            exec,
            capabilities,
            state: state.clone(),
            log,
            transport: Arc::new(Mutex::new(Some(transport))),
            child: Arc::new(Mutex::new(Some(child))),
            exit_tx,
        });

        let runtime_for_wait = runtime.clone();
        tokio::spawn(async move {
            let exit_code = {
                let mut guard = runtime_for_wait.child.lock().await;
                if let Some(child) = guard.as_mut() {
                    child
                        .wait()
                        .await
                        .map(|s| s.code().unwrap_or(-1))
                        .unwrap_or(-1)
                } else {
                    -1
                }
            };
            *runtime_for_wait.state.write().await = PluginState::Stopped;
            let _ = runtime_for_wait.exit_tx.send(exit_code).await;
        });

        Ok(runtime)
    }

    pub async fn transport(&self) -> Option<Arc<JsonRpcTransport>> {
        self.transport.lock().await.clone()
    }

    pub async fn shutdown(&self) {
        *self.state.write().await = PluginState::ShuttingDown;
        if let Some(t) = self.transport.lock().await.take() {
            t.shutdown().await;
        }
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
    }
}
