use serde::{Deserialize, Serialize};

/// Per-(channel, mix) mute state. `muted = true` tells the conf builder
/// to SKIP emitting the post-loopback for that pairing — no audio path
/// = silence in that mix. Lives in `tideline-core` so both base topology
/// and plugin contributors can reason about it. The SDK re-exports it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixMuteEntry {
    pub channel_name: String,
    pub mix_id: String,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewireableTag {
    pub channel_uuid: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LoadModuleHeader {
    /// Adapter-style entry: `{ factory = <name> args = { ... } }`. Lands in `context.objects`.
    Factory(String),
    /// Module-style entry: `{ name = <module> args = { ... } }`. Lands in `context.modules`.
    Named(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ArgValue {
    /// Bareword (no quotes): factory names, enum-like literals, booleans.
    Literal(String),
    /// Quoted string: node names, descriptions.
    Quoted(String),
    /// Nested `{ ... }` block: capture.props / playback.props.
    Group(Vec<(String, ArgValue)>),
    /// Array literal `[ v1, v2, ... ]`. Required for canonical channel
    /// positions: `audio.position = [ FL, FR ]`. The quoted-string form
    /// `"FL,FR"` is non-canonical and pipewire silently falls back to
    /// generic numeric port names (input_0/1 instead of input_FL/FR),
    /// breaking pw-link lookups by channel name.
    Array(Vec<ArgValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PipewireDirective {
    LoadModule {
        header: LoadModuleHeader,
        args: Vec<(String, ArgValue)>,
        rewireable_tag: Option<RewireableTag>,
    },
    RewireLoopback {
        target_tag: RewireableTag,
        new_capture_node: Option<String>,
        new_playback_node: Option<String>,
    },
    InsertNodeBefore {
        target_tag: RewireableTag,
        node_factory: String,
        args: Vec<(String, ArgValue)>,
    },
    DestroyModule {
        target_tag: RewireableTag,
    },
}
