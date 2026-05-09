//! Folds plugin contributions into a base directive list. Each extra is matched
//! by `RewireableTag` against tagged base directives; untagged `LoadModule`
//! contributions append to the list.

use super::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};

pub fn apply_contribution(
    base: Vec<PipewireDirective>,
    extras: &[PipewireDirective],
) -> Vec<PipewireDirective> {
    let mut out = base;
    for extra in extras {
        match extra {
            PipewireDirective::LoadModule { .. } => out.push(extra.clone()),
            PipewireDirective::DestroyModule { target_tag } => {
                out.retain(|d| !directive_has_tag(d, target_tag));
            }
            PipewireDirective::RewireLoopback {
                target_tag,
                new_capture_node,
                new_playback_node,
            } => {
                if let Some(idx) = out.iter().position(|d| directive_has_tag(d, target_tag)) {
                    if let PipewireDirective::LoadModule { args, .. } = &mut out[idx] {
                        if let Some(cap) = new_capture_node {
                            replace_target_object(args, "capture.props", cap);
                        }
                        if let Some(pb) = new_playback_node {
                            replace_target_object(args, "playback.props", pb);
                        }
                    }
                }
            }
            PipewireDirective::InsertNodeBefore {
                target_tag,
                node_factory,
                args,
            } => {
                if let Some(idx) = out.iter().position(|d| directive_has_tag(d, target_tag)) {
                    out.insert(
                        idx,
                        PipewireDirective::LoadModule {
                            header: LoadModuleHeader::Factory(node_factory.clone()),
                            args: args.clone(),
                            rewireable_tag: None,
                        },
                    );
                }
            }
        }
    }
    out
}

fn directive_has_tag(d: &PipewireDirective, tag: &RewireableTag) -> bool {
    matches!(
        d,
        PipewireDirective::LoadModule { rewireable_tag: Some(t), .. } if t == tag
    )
}

/// Update `target.object` for the entry under `key` (capture.props /
/// playback.props). Dual-mode: real generator emits `ArgValue::Group(...)`
/// here, so we recurse into the group and replace the inner `target.object`
/// pair. Tests may use a flat `Literal/Quoted`, in which case we replace the
/// whole leaf with `target.object=<new>` so callers can still assert on the
/// node name being present.
fn replace_target_object(args: &mut Vec<(String, ArgValue)>, key: &str, new_value: &str) {
    if let Some(pair) = args.iter_mut().find(|(k, _)| k == key) {
        match &mut pair.1 {
            ArgValue::Group(items) => {
                if let Some(inner) = items.iter_mut().find(|(k, _)| k == "target.object") {
                    inner.1 = ArgValue::Quoted(new_value.into());
                } else {
                    items.push(("target.object".into(), ArgValue::Quoted(new_value.into())));
                }
            }
            ArgValue::Literal(_) | ArgValue::Quoted(_) | ArgValue::Array(_) => {
                pair.1 = ArgValue::Literal(format!("target.object={}", new_value));
            }
        }
    } else {
        args.push((
            key.into(),
            ArgValue::Group(vec![(
                "target.object".into(),
                ArgValue::Quoted(new_value.into()),
            )]),
        ));
    }
}
