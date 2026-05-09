use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::HostClient;
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::EffectsState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditionPhase {
    Idle,
    Recording,
    Looping,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditionStatus {
    pub channel_uuid: Option<Uuid>,
    pub phase: AuditionPhase,
    pub has_sample: bool,
}

struct ActiveSession {
    channel_uuid: Uuid,
    phase: AuditionPhase,
    sample_path: Option<PathBuf>,
    /// True if `sample_path` was created by us (e.g. /tmp/tideline-audition-*.wav
    /// from a Record). False if the user supplied it via `load_file` — in
    /// that case we must NOT delete it on discard.
    owns_sample: bool,
    record_proc: Option<Child>,
    loop_cancel: Option<watch::Sender<bool>>,
    loop_task: Option<JoinHandle<()>>,
    loop_pid: Arc<AtomicI32>,
}

#[derive(Default)]
pub struct AuditionStore {
    inner: Mutex<Option<ActiveSession>>,
}

impl AuditionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn status_for(&self, channel: Uuid) -> AuditionStatus {
        let g = self.inner.lock().await;
        match g.as_ref() {
            Some(s) if s.channel_uuid == channel => AuditionStatus {
                channel_uuid: Some(s.channel_uuid),
                phase: s.phase,
                has_sample: s.sample_path.as_ref().is_some_and(|p| p.exists()),
            },
            _ => AuditionStatus {
                channel_uuid: None,
                phase: AuditionPhase::Idle,
                has_sample: false,
            },
        }
    }

    pub async fn start_record(
        &self,
        channel: Uuid,
        physical_source: &str,
    ) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        match g.as_ref() {
            Some(s) if s.phase != AuditionPhase::Idle => {
                return Err(format!(
                    "audition busy on channel {} (phase {:?})",
                    s.channel_uuid, s.phase
                ));
            }
            _ => {}
        }
        if physical_source.is_empty() {
            return Err("channel has no physical_source — cannot record".into());
        }

        let sample_path: PathBuf =
            std::env::temp_dir().join(format!("tideline-audition-{channel}.wav"));
        let _ = std::fs::remove_file(&sample_path);

        // Force --channels=1 so the WAV is unambiguously mono. Pulse
        // sometimes exposes a hardware-mono mic as a stereo source with
        // signal only on one channel, and recording stereo would
        // faithfully preserve that imbalance — making playback come out
        // on one ear after the chain. Forcing mono down-mixes at the
        // capture stage so the lone WAV channel always has the actual
        // mic signal.
        //
        // kill_on_drop makes sure that if the plugin process dies for
        // any reason, the orphan pw-cat is reaped instead of holding a
        // stream into the user's mic forever.
        let child = Command::new("pw-cat")
            .args([
                "--record",
                "--target",
                physical_source,
                "--channels=1",
            ])
            .arg(&sample_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("spawn pw-cat record failed: {e}"))?;

        info!(
            target: "tideline-effects::audition",
            %channel,
            path = %sample_path.display(),
            target = physical_source,
            "audition: recording started"
        );

        let mut child = child;
        if let Some(stderr) = child.stderr.take() {
            spawn_log_pipe(stderr, "pw-cat record");
        }
        // If pw-cat died immediately (e.g. unknown target), surface the
        // failure now instead of letting the user click "Stop" later
        // and find an empty file.
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "pw-cat exited immediately with {status}: target '{physical_source}' not usable?"
                ));
            }
            Ok(None) => {}
            Err(e) => {
                warn!(
                    target: "tideline-effects::audition",
                    error = %e,
                    "try_wait on pw-cat record failed"
                );
            }
        }

        *g = Some(ActiveSession {
            channel_uuid: channel,
            phase: AuditionPhase::Recording,
            sample_path: Some(sample_path),
            owns_sample: true,
            record_proc: Some(child),
            loop_cancel: None,
            loop_task: None,
            loop_pid: Arc::new(AtomicI32::new(0)),
        });
        Ok(())
    }

    /// Use a user-selected audio file as the audition source. Puts the
    /// session into Idle phase with sample_path set to the given path so
    /// the next start_loop will play this file instead of a freshly
    /// recorded sample. The file is referenced in place — not copied —
    /// so caller is responsible for keeping the path alive while looping.
    pub async fn load_file(&self, channel: Uuid, path: PathBuf) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        match g.as_ref() {
            Some(s) if s.phase != AuditionPhase::Idle => {
                return Err(format!(
                    "audition busy on channel {} (phase {:?})",
                    s.channel_uuid, s.phase
                ));
            }
            _ => {}
        }
        if !path.exists() {
            return Err(format!("file does not exist: {}", path.display()));
        }
        if !path.is_file() {
            return Err(format!("not a regular file: {}", path.display()));
        }
        info!(
            target: "tideline-effects::audition",
            %channel,
            path = %path.display(),
            "audition: loaded user file as source"
        );
        *g = Some(ActiveSession {
            channel_uuid: channel,
            phase: AuditionPhase::Idle,
            sample_path: Some(path),
            owns_sample: false,
            record_proc: None,
            loop_cancel: None,
            loop_task: None,
            loop_pid: Arc::new(AtomicI32::new(0)),
        });
        Ok(())
    }

    /// Copy the current audition sample to a user-chosen destination.
    /// Requires an Idle session with a sample_path set (post-record or
    /// post-load_file). Loop must be stopped first.
    pub async fn save_recording(&self, channel: Uuid, dest: PathBuf) -> Result<(), String> {
        let g = self.inner.lock().await;
        let sess = g.as_ref().ok_or_else(|| "no audition session".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if sess.phase != AuditionPhase::Idle {
            return Err(format!(
                "cannot save while phase is {:?} — stop the loop first",
                sess.phase
            ));
        }
        let src = sess
            .sample_path
            .as_ref()
            .ok_or_else(|| "no sample to save".to_string())?;
        if !src.exists() {
            return Err(format!("source missing on disk: {}", src.display()));
        }
        std::fs::copy(src, &dest)
            .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dest.display()))?;
        info!(
            target: "tideline-effects::audition",
            %channel,
            src = %src.display(),
            dest = %dest.display(),
            "audition: sample saved"
        );
        Ok(())
    }

    pub async fn stop_record(&self, channel: Uuid) -> Result<PathBuf, String> {
        let mut g = self.inner.lock().await;
        let sess = g
            .as_mut()
            .ok_or_else(|| "no active audition".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if sess.phase != AuditionPhase::Recording {
            return Err(format!("not recording (phase {:?})", sess.phase));
        }
        if let Some(mut child) = sess.record_proc.take() {
            terminate_child(&mut child).await;
        }
        sess.phase = AuditionPhase::Idle;
        let path = sess
            .sample_path
            .clone()
            .ok_or_else(|| "missing sample path".to_string())?;
        info!(
            target: "tideline-effects::audition",
            %channel,
            path = %path.display(),
            "audition: recording stopped"
        );
        Ok(path)
    }

    pub async fn start_loop(&self, channel: Uuid) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let sess = g
            .as_mut()
            .ok_or_else(|| "no audition session — record first".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if sess.phase != AuditionPhase::Idle {
            return Err(format!("cannot start loop in phase {:?}", sess.phase));
        }
        let path = sess
            .sample_path
            .clone()
            .ok_or_else(|| "no recorded sample".to_string())?;
        if !path.exists() {
            return Err(format!("sample missing on disk: {}", path.display()));
        }
        // Target the channel's in-process JACK client directly. This
        // avoids creating any new pipewire conf entries (which would
        // trigger a wireplumber/pipewire restart and thus an engine
        // recreate, which currently crashes inside libjack/pipewire on
        // jack_client_close).
        let target = crate::util::channel_jack_client(channel);
        let (tx, rx) = watch::channel(false);
        sess.loop_pid.store(0, Ordering::Relaxed);
        let pid_handle = sess.loop_pid.clone();
        let task = tokio::spawn(loop_player(path, target.clone(), rx, pid_handle));
        sess.loop_cancel = Some(tx);
        sess.loop_task = Some(task);
        sess.phase = AuditionPhase::Looping;
        info!(
            target: "tideline-effects::audition",
            %channel,
            target,
            "audition: loop started"
        );
        Ok(())
    }

    pub async fn stop_loop(&self, channel: Uuid) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let sess = g
            .as_mut()
            .ok_or_else(|| "no audition session".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if !matches!(sess.phase, AuditionPhase::Looping | AuditionPhase::Paused) {
            return Err(format!("not looping (phase {:?})", sess.phase));
        }
        // If we're paused, SIGCONT the child so it can receive SIGINT
        // from the loop_player's terminate path.
        let pid = sess.loop_pid.load(Ordering::Relaxed);
        if pid > 0 {
            unsafe {
                libc::kill(pid, libc::SIGCONT);
            }
        }
        if let Some(tx) = sess.loop_cancel.take() {
            let _ = tx.send(true);
        }
        if let Some(task) = sess.loop_task.take() {
            let _ = task.await;
        }
        sess.phase = AuditionPhase::Idle;
        sess.loop_pid.store(0, Ordering::Relaxed);
        info!(
            target: "tideline-effects::audition",
            %channel,
            "audition: loop stopped"
        );
        Ok(())
    }

    pub async fn pause_loop(&self, channel: Uuid) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let sess = g
            .as_mut()
            .ok_or_else(|| "no audition session".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if sess.phase != AuditionPhase::Looping {
            return Err(format!("cannot pause from phase {:?}", sess.phase));
        }
        let pid = sess.loop_pid.load(Ordering::Relaxed);
        if pid <= 0 {
            return Err("audition pid not yet known — try again".into());
        }
        let rc = unsafe { libc::kill(pid, libc::SIGSTOP) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("SIGSTOP failed: {err}"));
        }
        sess.phase = AuditionPhase::Paused;
        info!(
            target: "tideline-effects::audition",
            %channel,
            pid,
            "audition: loop paused"
        );
        Ok(())
    }

    pub async fn resume_loop(&self, channel: Uuid) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        let sess = g
            .as_mut()
            .ok_or_else(|| "no audition session".to_string())?;
        if sess.channel_uuid != channel {
            return Err(format!(
                "audition belongs to {}, not {channel}",
                sess.channel_uuid
            ));
        }
        if sess.phase != AuditionPhase::Paused {
            return Err(format!("cannot resume from phase {:?}", sess.phase));
        }
        let pid = sess.loop_pid.load(Ordering::Relaxed);
        if pid <= 0 {
            return Err("audition pid not tracked".into());
        }
        let rc = unsafe { libc::kill(pid, libc::SIGCONT) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("SIGCONT failed: {err}"));
        }
        sess.phase = AuditionPhase::Looping;
        info!(
            target: "tideline-effects::audition",
            %channel,
            pid,
            "audition: loop resumed"
        );
        Ok(())
    }

    pub async fn discard(&self, channel: Option<Uuid>) {
        let mut g = self.inner.lock().await;
        let take = match (g.as_ref(), channel) {
            (Some(s), Some(ch)) if s.channel_uuid != ch => false,
            (Some(_), _) => true,
            _ => false,
        };
        if !take {
            return;
        }
        if let Some(mut sess) = g.take() {
            // SIGCONT any paused playback so it can receive the
            // termination signal that follows.
            let pid = sess.loop_pid.load(Ordering::Relaxed);
            if pid > 0 {
                unsafe {
                    libc::kill(pid, libc::SIGCONT);
                }
            }
            if let Some(mut child) = sess.record_proc.take() {
                terminate_child(&mut child).await;
            }
            if let Some(tx) = sess.loop_cancel.take() {
                let _ = tx.send(true);
            }
            if let Some(task) = sess.loop_task.take() {
                let _ = task.await;
            }
            if let Some(p) = sess.sample_path.take() {
                if sess.owns_sample {
                    let _ = std::fs::remove_file(p);
                }
            }
            info!(
                target: "tideline-effects::audition",
                channel = %sess.channel_uuid,
                "audition: discarded"
            );
        }
    }
}

async fn loop_player(
    path: PathBuf,
    target: String,
    mut cancel: watch::Receiver<bool>,
    pid: Arc<AtomicI32>,
) {
    // Use a known node name so we can find pw-cat's output ports via
    // pw-link and explicitly link them to fx_node's input ports.
    // node.autoconnect=false stops pipewire's session manager from
    // routing pw-cat to the default sink (which is what was making
    // audio audible while the LV2 chain saw nothing).
    let cat_node = format!("tideline-audition-{}", uuid_from_target(&target));
    let mut consecutive_failures: u32 = 0;
    loop {
        if *cancel.borrow() {
            break;
        }
        let props = format!("node.name={cat_node},node.autoconnect=false");
        let spawn = Command::new("pw-cat")
            .args(["--playback", "-P", &props])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let mut child = match spawn {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    target: "tideline-effects::audition",
                    error = %e,
                    "pw-cat playback spawn failed"
                );
                consecutive_failures += 1;
                if consecutive_failures > 5 {
                    warn!(
                        target: "tideline-effects::audition",
                        "audition loop giving up after 5 consecutive spawn failures"
                    );
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                continue;
            }
        };

        if let Some(child_id) = child.id() {
            pid.store(child_id as i32, Ordering::Relaxed);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_pipe(stderr, "pw-cat playback");
        }

        // Link pw-cat's output ports into the channel's JACK client
        // input ports. Pipewire's session manager doesn't autoconnect
        // to JACK clients (media.class=null), so without this the
        // sample plays into the void.
        let cancel_for_link = cancel.clone();
        let cat_node_for_link = cat_node.clone();
        let target_for_link = target.clone();
        tokio::spawn(async move {
            if let Err(e) =
                link_audition_to_fx(&cat_node_for_link, &target_for_link, cancel_for_link).await
            {
                warn!(
                    target: "tideline-effects::audition",
                    error = %e,
                    "audition pw-link failed"
                );
            }
        });

        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    pid.store(0, Ordering::Relaxed);
                    terminate_child(&mut child).await;
                    break;
                }
            }
            res = child.wait() => {
                pid.store(0, Ordering::Relaxed);
                match res {
                    Ok(status) if status.success() => {
                        consecutive_failures = 0;
                    }
                    Ok(status) => {
                        consecutive_failures += 1;
                        warn!(
                            target: "tideline-effects::audition",
                            ?status,
                            target,
                            "pw-cat playback exited non-zero"
                        );
                        if consecutive_failures > 5 {
                            warn!(
                                target: "tideline-effects::audition",
                                "audition loop giving up after 5 consecutive failures"
                            );
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                    Err(e) => {
                        warn!(
                            target: "tideline-effects::audition",
                            error = %e,
                            "pw-cat playback wait failed"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }
    pid.store(0, Ordering::Relaxed);
    info!(
        target: "tideline-effects::audition",
        "audition loop player exited"
    );
}

/// Helper: derive a stable suffix from the fx_node name `tideline-fx-{uuid}`.
fn uuid_from_target(fx_node: &str) -> String {
    fx_node
        .strip_prefix("tideline-fx-")
        .unwrap_or(fx_node)
        .to_string()
}

async fn link_audition_to_fx(
    cat_node: &str,
    fx_node: &str,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let prefix = format!("{cat_node}:");
    let fx_inputs = ["in_FL", "in_FR"];
    for attempt in 0..30 {
        if *cancel.borrow_and_update() {
            return Ok(());
        }
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        let listing = Command::new("pw-link")
            .arg("-o")
            .stdin(Stdio::null())
            .output()
            .await;
        let stdout = match listing {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(_) => continue,
        };
        let mut cat_ports: Vec<String> = stdout
            .lines()
            .filter_map(|l| {
                let trimmed = l.trim();
                if trimmed.starts_with(&prefix) {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            })
            .collect();
        if cat_ports.is_empty() {
            continue;
        }
        cat_ports.sort();

        // Build link pairs. If pw-cat has fewer ports than fx_node has
        // input channels (i.e. mono source playing into a stereo JACK
        // client), duplicate the lone port across all fx inputs so the
        // chain processes a balanced stereo signal — matching what the
        // regular fx-pre loopback does via audio.position=FL,FR.
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (i, fx_in) in fx_inputs.iter().enumerate() {
            let port = cat_ports
                .get(i)
                .or_else(|| cat_ports.first())
                .cloned()
                .unwrap();
            pairs.push((port, format!("{fx_node}:{fx_in}")));
        }

        let mut all_linked = true;
        for (out, inp) in &pairs {
            let res = Command::new("pw-link")
                .arg(out)
                .arg(inp)
                .stdin(Stdio::null())
                .output()
                .await;
            match res {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if !stderr.contains("File exists") {
                        all_linked = false;
                    }
                }
                Err(_) => all_linked = false,
            }
        }
        if all_linked {
            info!(
                target: "tideline-effects::audition",
                cat_node,
                fx_node,
                links = ?pairs,
                "audition: linked pw-cat → fx_node"
            );
            return Ok(());
        }
    }
    Err("exhausted retries linking audition to fx_node".into())
}

async fn terminate_child(child: &mut Child) {
    use tokio::time::{timeout, Duration};
    if let Some(pid) = child.id() {
        unsafe {
            // SIGCONT in case the child is currently stopped (paused
            // audition); SIGINT alone will queue but not deliver while
            // the process is in T state.
            libc::kill(pid as i32, libc::SIGCONT);
            libc::kill(pid as i32, libc::SIGINT);
        }
    }
    if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

fn spawn_log_pipe(stderr: tokio::process::ChildStderr, label: &'static str) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    tracing::info!(
                        target: "tideline-effects::audition",
                        label,
                        line = %line,
                        "child stderr"
                    );
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
}

#[derive(Debug, Deserialize)]
struct RecordStartParams {
    channel_uuid: Uuid,
    physical_source: String,
}

#[derive(Debug, Deserialize)]
struct ChannelOnlyParams {
    channel_uuid: Uuid,
}

fn invalid_params(msg: impl Into<String>) -> RpcError {
    RpcError {
        code: error_codes::INVALID_PARAMS,
        message: msg.into(),
        data: None,
    }
}

fn internal(msg: impl Into<String>) -> RpcError {
    RpcError {
        code: error_codes::INTERNAL_ERROR,
        message: msg.into(),
        data: None,
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(
    params: Option<Value>,
    method: &str,
) -> Result<T, RpcError> {
    let raw = params.ok_or_else(|| invalid_params(format!("{method}: missing params")))?;
    serde_json::from_value(raw).map_err(|e| invalid_params(format!("{method}: parse: {e}")))
}

async fn poke_rebuild(host: &Arc<HostClient>, reason: &str) {
    if let Err(e) = host
        .event_publish(
            "tideline-effects:rack_changed",
            serde_json::json!({ "reason": reason }),
        )
        .await
    {
        warn!(?e, reason, "rack_changed publish failed");
    }
}

pub async fn handle_record_start(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: RecordStartParams = parse_params(params, "effects.audition_record_start")?;
    state
        .audition
        .start_record(p.channel_uuid, &p.physical_source)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_record_stop(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_record_stop")?;
    let path = state
        .audition
        .stop_record(p.channel_uuid)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({
        "ok": true,
        "sample_path": path.to_string_lossy(),
    }))
}


#[derive(Deserialize)]
struct LoadFileParams {
    channel_uuid: Uuid,
    path: String,
}

pub async fn handle_load_file(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: LoadFileParams = parse_params(params, "effects.audition_load_file")?;
    let path = PathBuf::from(p.path);
    state
        .audition
        .load_file(p.channel_uuid, path)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct SaveRecordingParams {
    channel_uuid: Uuid,
    path: String,
}

pub async fn handle_save_recording(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: SaveRecordingParams = parse_params(params, "effects.audition_save_recording")?;
    let dest = PathBuf::from(p.path);
    state
        .audition
        .save_recording(p.channel_uuid, dest)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_loop_start(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_loop_start")?;
    state
        .audition
        .start_loop(p.channel_uuid)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_loop_stop(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_loop_stop")?;
    state
        .audition
        .stop_loop(p.channel_uuid)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}


pub async fn handle_loop_pause(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_loop_pause")?;
    state
        .audition
        .pause_loop(p.channel_uuid)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_loop_resume(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_loop_resume")?;
    state
        .audition
        .resume_loop(p.channel_uuid)
        .await
        .map_err(internal)?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_discard(
    state: &Arc<EffectsState>,
    host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_discard")?;
    // Snapshot whether we were looping so we know if we need to poke
    // pipewire to tear down the audition source.
    let was_looping = state
        .audition
        .status_for(p.channel_uuid)
        .await
        .phase
        == AuditionPhase::Looping;
    state.audition.discard(Some(p.channel_uuid)).await;
    if was_looping {
        poke_rebuild(&host, "audition_discard").await;
    }
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn handle_status(
    state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let p: ChannelOnlyParams = parse_params(params, "effects.audition_state")?;
    let s = state.audition.status_for(p.channel_uuid).await;
    serde_json::to_value(s).map_err(|e| internal(format!("serialize: {e}")))
}
