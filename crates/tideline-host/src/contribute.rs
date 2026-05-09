use tideline_core::pipewire::directive::{PipewireDirective, RewireableTag};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PluginPriority(pub i32);

#[derive(Debug, Clone)]
pub struct PluginContribution {
    pub plugin_id: String,
    pub priority: PluginPriority,
    pub directives: Vec<PipewireDirective>,
}

pub fn resolve_collisions(mut input: Vec<PluginContribution>) -> Vec<PluginContribution> {
    input.sort_by_key(|c| std::cmp::Reverse(c.priority));
    let mut claimed: Vec<RewireableTag> = Vec::new();
    let mut out: Vec<PluginContribution> = Vec::new();
    for contrib in input {
        let mut keep = Vec::new();
        for d in &contrib.directives {
            let touched = touched_tag(d);
            match touched {
                Some(tag) if claimed.contains(&tag) => continue,
                Some(tag) => {
                    claimed.push(tag);
                    keep.push(d.clone());
                }
                None => keep.push(d.clone()),
            }
        }
        if !keep.is_empty() {
            out.push(PluginContribution {
                directives: keep,
                ..contrib
            });
        }
    }
    out
}

fn touched_tag(d: &PipewireDirective) -> Option<RewireableTag> {
    match d {
        PipewireDirective::DestroyModule { target_tag } => Some(target_tag.clone()),
        PipewireDirective::RewireLoopback { target_tag, .. } => Some(target_tag.clone()),
        PipewireDirective::InsertNodeBefore { target_tag, .. } => Some(target_tag.clone()),
        PipewireDirective::LoadModule { .. } => None,
    }
}

pub async fn collect_pipewire_contributions(
    registry: &crate::registry::PluginRegistry,
    cfg: &tideline_core::model::AppConfig,
    mix_mutes: &[tideline_sdk::contribute::MixMuteEntry],
) -> Vec<PluginContribution> {
    use tideline_sdk::contribute::{
        PipewireContributeRequest, PipewireContributeResponse, SerializedAppConfig,
    };

    let cfg_value = match serde_json::to_value(cfg) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "collect_pipewire_contributions: serialize AppConfig failed");
            eprintln!("[contribute] serialize AppConfig failed: {e}");
            return Vec::new();
        }
    };

    let plugin_ids = registry
        .plugins_with_capability(tideline_sdk::Capability::PipewireContribute)
        .await;
    eprintln!(
        "[contribute] plugins_with_capability(pipewire.contribute) = {:?}",
        plugin_ids
    );

    let mut out = Vec::with_capacity(plugin_ids.len());
    for plugin_id in plugin_ids {
        let req = PipewireContributeRequest {
            config: SerializedAppConfig {
                json: cfg_value.clone(),
            },
            mix_mutes: mix_mutes.to_vec(),
        };
        let params = match serde_json::to_value(&req) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(plugin = %plugin_id, error = %e, "serialize contribute request failed");
                eprintln!("[contribute] {plugin_id}: serialize req failed: {e}");
                continue;
            }
        };
        let raw = match registry
            .send_request(&plugin_id, "pipewire.contribute_request", params)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(plugin = %plugin_id, error = %e, "pipewire.contribute_request failed");
                eprintln!("[contribute] {plugin_id}: send_request failed: {e}");
                continue;
            }
        };
        let resp: PipewireContributeResponse = match serde_json::from_value(raw.clone()) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(plugin = %plugin_id, error = %e, "parse contribute response failed");
                eprintln!(
                    "[contribute] {plugin_id}: parse response failed: {e}; raw = {}",
                    raw
                );
                continue;
            }
        };
        eprintln!(
            "[contribute] {plugin_id}: got {} directives",
            resp.directives.len()
        );
        if resp.directives.is_empty() {
            continue;
        }
        out.push(PluginContribution {
            plugin_id,
            priority: PluginPriority(0),
            directives: resp.directives,
        });
    }
    out
}
