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
