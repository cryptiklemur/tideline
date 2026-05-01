use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewireableTag {
    pub channel_uuid: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadModuleHeader {
    /// Adapter-style entry: `{ factory = <name> args = { ... } }`. Lands in `context.objects`.
    Factory(String),
    /// Module-style entry: `{ name = <module> args = { ... } }`. Lands in `context.modules`.
    Named(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgValue {
    /// Bareword (no quotes): factory names, enum-like literals, booleans.
    Literal(String),
    /// Quoted string: node names, descriptions, channel positions.
    Quoted(String),
    /// Nested `{ ... }` block: capture.props / playback.props.
    Group(Vec<(String, ArgValue)>),
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
