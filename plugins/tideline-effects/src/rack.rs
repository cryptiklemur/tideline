use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tideline_sdk::client::HostClient;
use tideline_sdk::rpc::{error_codes, RpcError};
use uuid::Uuid;

use crate::chain_ops;
use crate::discovery::{self, Category};
use crate::host::PluginInfo;
use crate::effect::{Effect, PluginFormat};
use crate::iframe_bridge::persist_channel;
use crate::overlay_render::channel_card_tree;
use crate::state::EffectsState;

const RACK_CHANGED_TOPIC: &str = "tideline-effects:rack_changed";

fn parse_params<T: for<'de> Deserialize<'de>>(
    params: Option<Value>,
    method: &str,
) -> Result<T, RpcError> {
    let value = params.ok_or_else(|| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("{method}: missing params"),
        data: None,
    })?;
    serde_json::from_value(value).map_err(|e| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("{method}: {e}"),
        data: None,
    })
}

fn category_label(c: Category) -> &'static str {
    match c {
        Category::Eq => "EQ",
        Category::Dynamics => "Dynamics",
        Category::Reverb => "Reverb",
        Category::Modulation => "Modulation",
        Category::Utility => "Utility",
        Category::Other => "Other",
    }
}

fn format_label(f: PluginFormat) -> &'static str {
    match f {
        PluginFormat::Lv2 => "LV2",
        PluginFormat::Vst3 => "VST3",
        PluginFormat::Vst2 => "VST2",
    }
}

pub async fn list_catalog(state: &Arc<EffectsState>, params: Option<Value>) -> Result<Value, RpcError> {
    // Optional `channel_kind: "physical_input" | "input" | "output"` lets the
    // recommender bias toward mono-variant plugins for mics and stereo for
    // everything else. Missing/unknown kinds default to stereo since most
    // channels are.
    let channel_mono = params
        .as_ref()
        .and_then(|v| v.get("channel_kind"))
        .and_then(|v| v.as_str())
        .map(|k| k == "physical_input")
        .unwrap_or(false);

    let cache = discovery::ensure_cached(state).await;
    let recommended = collect_recommended(&cache.plugins, channel_mono);
    let recommended_uris: std::collections::HashSet<&str> =
        recommended.iter().map(|p| p.uri.as_str()).collect();

    let mut groups: Vec<(Category, Vec<&PluginInfo>)> = vec![
        (Category::Eq, vec![]),
        (Category::Dynamics, vec![]),
        (Category::Reverb, vec![]),
        (Category::Modulation, vec![]),
        (Category::Utility, vec![]),
        (Category::Other, vec![]),
    ];
    for info in &cache.plugins {
        let cat = discovery::categorize(&info.category);
        if let Some(slot) = groups.iter_mut().find(|(c, _)| *c == cat) {
            slot.1.push(info);
        }
    }
    let groups_json: Vec<Value> = groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(cat, items)| {
            json!({
                "category": format!("{cat:?}").to_lowercase(),
                "label": category_label(cat),
                "items": items.iter().map(|p| json!({
                    "uri": p.uri,
                    "name": p.name,
                    "vendor": p.vendor,
                    "format": format_label(p.format),
                    "category": category_label(discovery::categorize(&p.category)),
                    "recommended": recommended_uris.contains(p.uri.as_str()),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let recommended_json: Vec<Value> = recommended
        .iter()
        .map(|p| {
            json!({
                "uri": p.uri,
                "name": p.name,
                "vendor": p.vendor,
                "format": format_label(p.format),
                "category": category_label(discovery::categorize(&p.category)),
                "recommended": true,
            })
        })
        .collect();
    Ok(json!({
        "groups": groups_json,
        "recommended": recommended_json,
    }))
}

#[derive(Debug, Deserialize)]
struct RenderRackParams {
    channel_uuid: Uuid,
}

fn recommended_uri_needles(channel_mono: bool) -> Vec<&'static str> {
    // Mono channels (mics) want mono-variant plugins; stereo channels (Input
    // virtual sources, Output mix buses) want stereo variants. Plugins that
    // only ship as stereo (most Calf, room_builder, etc.) work on either kind
    // since the in-process effect chain runs stereo internally regardless.
    if channel_mono {
        vec![
            // Noise suppression — werman rnnoise. ML-based, voice-tuned,
            // ~10ms latency; the de-facto choice on linux for mic cleanup.
            "werman/noise-suppression-for-voice#mono",
            // LSP mono variants
            "para_equalizer_x16_mono",
            "Fil4Mono",
            "sc_compressor_mono",
            "compressor_mono",
            "expander_mono",
            "gate_mono",
            "limiter_mono",
            // Calf (stereo-only but fine on mono channels)
            "calf.sourceforge.net/plugins/Compressor",
            "calf.sourceforge.net/plugins/Gate",
            "calf.sourceforge.net/plugins/Deesser",
            "calf.sourceforge.net/plugins/Limiter",
        ]
    } else {
        vec![
            // Noise suppression — werman rnnoise stereo variant.
            "werman/noise-suppression-for-voice#stereo",
            // LSP stereo variants
            "para_equalizer_x16_stereo",
            "Equalizer8Band",
            "Fil4Stereo",
            "sc_compressor_stereo",
            "compressor_stereo",
            "darc",
            // Calf
            "calf.sourceforge.net/plugins/Compressor",
            "calf.sourceforge.net/plugins/Gate",
            "calf.sourceforge.net/plugins/Limiter",
            "calf.sourceforge.net/plugins/Deesser",
            "calf.sourceforge.net/plugins/ReverbIR",
            "calf.sourceforge.net/plugins/Reverb",
            "room_builder_stereo",
            "calf.sourceforge.net/plugins/Phaser",
            "calf.sourceforge.net/plugins/Chorus",
            "calf.sourceforge.net/plugins/MultiChorus",
            "balance",
        ]
    }
}

fn collect_recommended<'a>(plugins: &'a [PluginInfo], channel_mono: bool) -> Vec<&'a PluginInfo> {
    let mut out: Vec<&PluginInfo> = Vec::new();
    let needles = recommended_uri_needles(channel_mono);
    // Hide opposite-channel-count LSP variants from "Recommended" so a mono
    // mic doesn't get pushed a `_stereo` plugin and vice versa. The full
    // catalog still lists them; this only affects the curated set at top.
    let opposite_suffix = if channel_mono { "_stereo" } else { "_mono" };
    for needle in needles {
        if let Some(p) = plugins.iter().find(|p| {
            (p.uri.contains(needle) || p.name.contains(needle))
                && !p.uri.contains(opposite_suffix)
                && !out.iter().any(|q| q.uri == p.uri)
        }) {
            out.push(p);
        }
    }
    out
}

/// Returns true if any installed plugin's uri/name/vendor contains any of the
/// case-insensitive needles. Used to suppress recommendations the user already has.
fn collection_installed(plugins: &[PluginInfo], needles: &[&str]) -> bool {
    plugins.iter().any(|p| {
        let uri = p.uri.to_lowercase();
        let name = p.name.to_lowercase();
        let vendor = p.vendor.to_lowercase();
        needles.iter().any(|n| {
            let nl = n.to_lowercase();
            uri.contains(&nl) || name.contains(&nl) || vendor.contains(&nl)
        })
    })
}

fn recommended_install_children(plugins: &[PluginInfo]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let has_reaplugs = collection_installed(
        plugins,
        &[
            "reacomp", "reaeq", "reagate", "reaxcomp", "reaverbate", "reapitch", "reafir",
            "readelay", "reatune", "reaplugs", "cockos",
        ],
    );
    if !has_reaplugs {
        out.push(json!({
            "kind": "row",
            "id": "np-rec-reaplugs",
            "gap": 3,
            "align": "start",
            "children": [
                { "kind": "icon", "id": "np-rec-reaplugs-icon", "icon": { "name": "star" }, "size": 14 },
                {
                    "kind": "col",
                    "id": "np-rec-reaplugs-col",
                    "gap": 1,
                    "children": [
                        { "kind": "label", "id": "np-rec-reaplugs-name", "text": "ReaPlugs FX Suite — free pro-quality plugins" },
                        { "kind": "label", "id": "np-rec-reaplugs-desc", "text": "ReaComp, ReaEQ, ReaGate, ReaXcomp, ReaFir, ReaVerbate, ReaPitch, ReaDelay. Industry-standard for voice and streaming.", "muted": true },
                        { "kind": "label", "id": "np-rec-reaplugs-url", "text": "Download: https://www.reaper.fm/reaplugs/", "muted": true }
                    ]
                }
            ]
        }));
    }
    let has_reajs = collection_installed(plugins, &["reajs", "jsfx"]);
    if !has_reajs {
        out.push(json!({
            "kind": "row",
            "id": "np-rec-reapack",
            "gap": 3,
            "align": "start",
            "children": [
                { "kind": "icon", "id": "np-rec-reapack-icon", "icon": { "name": "star" }, "size": 14 },
                {
                    "kind": "col",
                    "id": "np-rec-reapack-col",
                    "gap": 0,
                    "children": [
                        { "kind": "label", "id": "np-rec-reapack-name", "text": "ReaPack + ReaJS — thousands of free JSFX effects" },
                        { "kind": "label", "id": "np-rec-reapack-desc", "text": "ReaPack is the REAPER community package manager. Pair it with ReaJS (JSFX VST wrapper, included in ReaPlugs) to access community JSFX effects.", "muted": true },
                        { "kind": "label", "id": "np-rec-reapack-url", "text": "Get them: https://reapack.com  ·  https://www.reaper.fm/reaplugs/ (includes ReaJS)", "muted": true }
                    ]
                }
            ]
        }));
    }
    out
}

/// Build the "Other open-source collections" card children, filtering out anything already installed.
fn other_install_children(plugins: &[PluginInfo]) -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    let has_lsp = collection_installed(plugins, &["lsp-plug.in", "lsp_plug", "lsp plug"]);
    let has_calf = collection_installed(plugins, &["calf.sourceforge", "calf studio", "calfbox"]);
    let has_x42 = collection_installed(plugins, &["gareus.org", "x42"]);
    let has_swh = collection_installed(plugins, &["swh-plugins", "ladspa.org", "swh "]);

    if !has_lsp {
        items.push(json!({ "kind": "label", "id": "np-install-lsp", "text": "LSP Plugins — EQ, dynamics, reverb, and more (package: lsp-plugins)" }));
    }
    if !has_calf {
        items.push(json!({ "kind": "label", "id": "np-install-calf", "text": "Calf Studio Gear — EQ, compressors, reverb (package: calf or calf-plugins)" }));
    }
    if !has_x42 {
        items.push(json!({ "kind": "label", "id": "np-install-x42", "text": "x42 Plugins — utilities, metering, FIL EQ (package: x42-plugins)" }));
    }
    if !has_swh {
        items.push(json!({ "kind": "label", "id": "np-install-swh", "text": "SWH Plugins — classic LADSPA-derived set (package: swh-plugins)" }));
    }

    // Only show the install hints if at least one collection is missing.
    if !items.is_empty() {
        let mut missing_pkgs_arch: Vec<&str> = Vec::new();
        let mut missing_pkgs_deb: Vec<&str> = Vec::new();
        if !has_lsp {
            missing_pkgs_arch.push("lsp-plugins");
            missing_pkgs_deb.push("lsp-plugins");
        }
        if !has_calf {
            missing_pkgs_arch.push("calf");
            missing_pkgs_deb.push("calf-plugins");
        }
        if !has_x42 {
            missing_pkgs_arch.push("x42-plugins");
            missing_pkgs_deb.push("x42-plugins");
        }
        if !has_swh {
            missing_pkgs_arch.push("swh-plugins");
            missing_pkgs_deb.push("swh-plugins");
        }
        items.push(json!({
            "kind": "label",
            "id": "np-install-arch",
            "text": format!("Arch: sudo pacman -S {}", missing_pkgs_arch.join(" ")),
            "muted": true,
        }));
        items.push(json!({
            "kind": "label",
            "id": "np-install-deb",
            "text": format!("Debian / Ubuntu: sudo apt install {}", missing_pkgs_deb.join(" ")),
            "muted": true,
        }));
    }
    items
}

/// Top-level installation guidance (recommended + other collections), filtered against
/// what's already installed. Returns the children of a wrapping `col`.
fn installation_guidance_children(plugins: &[PluginInfo]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let recs = recommended_install_children(plugins);
    if !recs.is_empty() {
        out.push(json!({
            "kind": "section",
            "id": "np-recommended",
            "title": "Highly recommended",
            "subtitle": "Free, pro-quality plugins from the makers of REAPER — widely used by streamers and audio engineers",
            "variant": "card",
            "gap": 3,
            "children": recs,
        }));
    }
    let others = other_install_children(plugins);
    if !others.is_empty() {
        out.push(json!({
            "kind": "section",
            "id": "np-install",
            "title": "Other open-source collections",
            "subtitle": "Native LV2 / VST3 sets — install via your package manager",
            "variant": "card",
            "gap": 3,
            "children": others,
        }));
    }
    out
}

pub async fn render_rack(
    state: &Arc<EffectsState>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let req: RenderRackParams = parse_params(params, "effects.render_rack")?;
    let chain = state.chain_order(req.channel_uuid).await;
    let chain_bypassed = state.get_chain_bypass(req.channel_uuid).await;

    let catalog_snapshot = state.catalog_clone().await;
    let has_ui_for = |format: crate::host::Format, uri: &str| -> bool {
        catalog_snapshot
            .iter()
            .any(|i| i.format == format && i.uri == uri && i.has_custom_ui)
    };

    let mut effect_rows: Vec<Value> = Vec::new();
    for eid in &chain {
        let slot_opt = state
            .effects
            .lock()
            .await
            .get(&(req.channel_uuid, *eid))
            .cloned();
        let Some(slot) = slot_opt else { continue };
        let bypassed = slot.effect.bypassed;
        let mut children: Vec<Value> = vec![
            json!({
                "kind": "icon",
                "id": format!("grip:{eid}"),
                "icon": { "name": "grip" },
                "size": 14,
            }),
            json!({
                "kind": "toggle",
                "id": format!("bypass:{eid}"),
                "value": !bypassed,
            }),
            json!({
                "kind": "col",
                "id": format!("name-col:{eid}"),
                "gap": 0,
                "children": [
                    {
                        "kind": "label",
                        "id": format!("name:{eid}"),
                        "text": slot.effect.display_name,
                    },
                    {
                        "kind": "label",
                        "id": format!("uri:{eid}"),
                        "text": slot.effect.uri,
                        "muted": true,
                    }
                ],
            }),
            json!({ "kind": "spacer", "id": format!("sp:{eid}") }),
            json!({
                "kind": "badge",
                "id": format!("fmt:{eid}"),
                "text": format_label(slot.effect.format),
            }),
        ];
        if has_ui_for(slot.effect.format, &slot.effect.uri) {
            children.push(json!({
                "kind": "button",
                "id": format!("open:{eid}"),
                "text": "",
                "icon": { "name": "settings" },
                "variant": "ghost",
            }));
        }
        children.push(json!({
            "kind": "button",
            "id": format!("remove:{eid}"),
            "text": "",
            "icon": { "name": "trash" },
            "variant": "ghost",
        }));
        let row = json!({
            "kind": "row",
            "id": format!("rack-row:{eid}"),
            "gap": 3,
            "align": "center",
            "variant": "card",
            "muted": bypassed || chain_bypassed,
            "pad": 3,
            "draggable": true,
            "drop_group": "fx-chain",
            "children": children,
        });
        effect_rows.push(row);
    }

    let cache = discovery::ensure_cached(state).await;
    let has_plugins = !cache.plugins.is_empty();
    let guidance_children = installation_guidance_children(&cache.plugins);

    let action_children: Vec<Value> = if has_plugins {
        vec![
            json!({
                "kind": "button",
                "id": "rescan",
                "text": "",
                "icon": { "name": "refresh" },
                "variant": "ghost",
                "tooltip": "Rescan plugin folders for newly installed LV2/VST plugins",
            }),
            json!({
                "kind": "button",
                "id": "open_plugin_picker",
                "text": "Add effect",
                "icon": { "name": "plus" },
                "variant": "primary",
            }),
        ]
    } else {
        vec![json!({
            "kind": "button",
            "id": "rescan",
            "text": "Rescan",
            "icon": { "name": "refresh" },
            "variant": "soft",
            "tooltip": "Rescan plugin folders for newly installed LV2/VST plugins",
        })]
    };

    let action_group = json!({
        "kind": "row",
        "id": "rack-toolbar-actions",
        "gap": 2,
        "align": "center",
        "children": action_children,
    });

    let toolbar = json!({
        "kind": "row",
        "id": "rack-toolbar",
        "gap": 3,
        "align": "center",
        "variant": "card",
        "pad": 3,
        "children": [
            {
                "kind": "toggle",
                "id": "chain_bypass",
                "value": !chain_bypassed,
                "style": "power",
                "size": "md",
                "tooltip": "Effects chain power. When off, audio bypasses the chain entirely.",
            },
            {
                "kind": "col",
                "id": "rack-toolbar-meta",
                "gap": 0,
                "children": [
                    {
                        "kind": "label",
                        "id": "rack-toolbar-title",
                        "text": "Effects chain",
                    },
                    {
                        "kind": "label",
                        "id": "rack-toolbar-state",
                        "text": if chain_bypassed { "Bypassed" } else { "Active" },
                        "muted": true,
                    }
                ],
            },
            { "kind": "spacer", "id": "rack-toolbar-sp" },
            action_group
        ],
    });

    let body = if !has_plugins {
        let home = dirs::home_dir();
        let mut path_lines: Vec<(String, Vec<String>)> = vec![
            ("LV2".to_string(), Vec::new()),
            ("VST3".to_string(), Vec::new()),
            ("VST2".to_string(), Vec::new()),
        ];
        for (fmt, path) in discovery::plugin_search_paths() {
            let display = if let Some(h) = &home {
                if let Ok(rel) = path.strip_prefix(h) {
                    format!("~/{}", rel.display())
                } else {
                    path.display().to_string()
                }
            } else {
                path.display().to_string()
            };
            let key = format_label(fmt).to_string();
            if let Some(slot) = path_lines.iter_mut().find(|(k, _)| *k == key) {
                slot.1.push(display);
            }
        }
        let path_rows: Vec<Value> = path_lines
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(fmt, paths)| {
                let lower = fmt.to_lowercase();
                json!({
                    "kind": "row",
                    "id": format!("np-path-{lower}"),
                    "gap": 3,
                    "align": "start",
                    "children": [
                        { "kind": "badge", "id": format!("np-path-{lower}-badge"), "text": fmt, "variant": "neutral" },
                        {
                            "kind": "label",
                            "id": format!("np-path-{lower}-text"),
                            "text": paths.join("    ·    "),
                            "muted": true,
                        }
                    ],
                })
            })
            .collect();

        let mut np_children: Vec<Value> = vec![
            json!({ "kind": "spacer", "id": "np-top", "size": 8 }),
            json!({ "kind": "row", "id": "np-hero-row", "align": "center", "children": [
                { "kind": "spacer", "id": "np-hero-l" },
                { "kind": "icon", "id": "np-hero-icon", "icon": { "name": "package-open" }, "size": 40 },
                { "kind": "spacer", "id": "np-hero-r" }
            ]}),
            json!({ "kind": "row", "id": "np-title-row", "align": "center", "children": [
                { "kind": "spacer", "id": "np-title-l" },
                { "kind": "heading", "id": "np-title", "text": "No effect plugins found", "level": 4 },
                { "kind": "spacer", "id": "np-title-r" }
            ]}),
            json!({ "kind": "row", "id": "np-sub-row", "align": "center", "children": [
                { "kind": "spacer", "id": "np-sub-l" },
                { "kind": "label", "id": "np-sub", "text": "Tideline scans for LV2, CLAP, VST3, and VST2 plugins. Install some, then rescan.", "muted": true },
                { "kind": "spacer", "id": "np-sub-r" }
            ]}),
            json!({
                "kind": "section",
                "id": "np-paths",
                "title": "Where Tideline looks",
                "subtitle": "Drop plugins into any of these directories",
                "variant": "card",
                "gap": 3,
                "children": path_rows,
            }),
        ];
        np_children.extend(guidance_children.clone());
        np_children.push(json!({ "kind": "row", "id": "np-cta-row", "align": "center", "children": [
            { "kind": "spacer", "id": "np-cta-l" },
            { "kind": "button", "id": "rescan", "text": "Rescan now", "icon": { "name": "refresh" }, "variant": "primary" },
            { "kind": "spacer", "id": "np-cta-r" }
        ]}));
        np_children.push(json!({ "kind": "spacer", "id": "np-bot", "size": 8 }));

        json!({
            "kind": "col",
            "id": "rack-no-plugins",
            "gap": 4,
            "children": np_children,
        })
    } else if effect_rows.is_empty() {
        json!({
            "kind": "col",
            "id": "rack-empty",
            "gap": 3,
            "children": [
                { "kind": "spacer", "id": "rack-empty-top", "size": 12 },
                {
                    "kind": "row",
                    "id": "rack-empty-icon-row",
                    "align": "center",
                    "children": [
                        { "kind": "spacer", "id": "rack-empty-icon-l" },
                        { "kind": "icon", "id": "rack-empty-icon", "icon": { "name": "fx" }, "size": 40 },
                        { "kind": "spacer", "id": "rack-empty-icon-r" }
                    ],
                },
                {
                    "kind": "row",
                    "id": "rack-empty-title-row",
                    "align": "center",
                    "children": [
                        { "kind": "spacer", "id": "rack-empty-title-l" },
                        { "kind": "heading", "id": "rack-empty-title", "text": "No effects yet", "level": 4 },
                        { "kind": "spacer", "id": "rack-empty-title-r" }
                    ],
                },
                {
                    "kind": "row",
                    "id": "rack-empty-sub-row",
                    "align": "center",
                    "children": [
                        { "kind": "spacer", "id": "rack-empty-sub-l" },
                        { "kind": "label", "id": "rack-empty-sub", "text": "Use Add effect above to insert one into the chain.", "muted": true },
                        { "kind": "spacer", "id": "rack-empty-sub-r" }
                    ],
                },
                { "kind": "spacer", "id": "rack-empty-bot", "size": 12 }
            ],
        })
    } else {
        json!({
            "kind": "col",
            "id": "rack-list",
            "gap": 2,
            "children": effect_rows,
        })
    };

    // Build root children. When plugins are installed but more recommendations remain,
    // append a collapsible-style "Get more plugins" section after the body so the
    // guidance is reachable without an empty state.
    let mut root_children: Vec<Value> = vec![toolbar, body];
    if has_plugins && !guidance_children.is_empty() {
        let mut more_children: Vec<Value> = Vec::with_capacity(guidance_children.len() + 1);
        more_children.push(json!({
            "kind": "label",
            "id": "more-plugins-sub",
            "text": "Add more plugin sources to expand your effects library.",
            "muted": true,
        }));
        more_children.extend(guidance_children);
        root_children.push(json!({
            "kind": "section",
            "id": "more-plugins",
            "title": "Get more plugins",
            "variant": "card",
            "gap": 3,
            "children": more_children,
        }));
    }

    Ok(json!({
        "kind": "col",
        "id": "rack-root",
        "gap": 4,
        "children": root_children,
    }))
}

#[derive(Debug, Deserialize)]
struct RackEventParams {
    channel_uuid: Uuid,
    node_id: String,
    #[serde(default)]
    value: Value,
}

pub async fn handle_event(
    state: &Arc<EffectsState>,
    host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let evt: RackEventParams = parse_params(params, "effects.rack_event")?;
    // Menu buttons (e.g. "add_effect") emit their own id as node_id and the
    // selected item id as a string value. Resolve those to the real action.
    let effective_node_id: String = match (evt.node_id.as_str(), &evt.value) {
        ("add_effect", Value::String(s)) => s.clone(),
        _ => evt.node_id.clone(),
    };
    let (kind, rest) = match effective_node_id.split_once(':') {
        Some(p) => p,
        None => (effective_node_id.as_str(), ""),
    };

    match kind {
        "add" => {
            let uri = rest.to_string();
            let cache = discovery::ensure_cached(state).await;
            let info = cache
                .plugins
                .iter()
                .find(|p| p.uri == uri)
                .cloned()
                .ok_or_else(|| RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: format!("unknown plugin uri {uri}"),
                    data: None,
                })?;
            let effect = Effect {
                id: Uuid::new_v4(),
                format: info.format,
                uri: info.uri,
                display_name: info.name,
                bypassed: false,
                state_b64: None,
            };
            chain_ops::add_effect(state.clone(), evt.channel_uuid, effect)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("add_effect: {e}"),
                    data: None,
                })?;
        }
        "remove" => {
            let effect_id: Uuid = rest.parse().map_err(|_| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("bad effect id {rest}"),
                data: None,
            })?;
            chain_ops::remove_effect(state.clone(), evt.channel_uuid, effect_id)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("remove_effect: {e}"),
                    data: None,
                })?;
        }
        "rack-row" => {
            // Drop event: a row was dragged onto another row. Reorder
            // by inserting the dragged effect immediately before the
            // drop target's current position.
            let drop_type = evt.value.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if drop_type != "drop" {
                return Err(RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: format!("unsupported rack-row event type {drop_type}"),
                    data: None,
                });
            }
            let target_eid: Uuid = rest.parse().map_err(|_| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("bad target effect id {rest}"),
                data: None,
            })?;
            let from_id = evt
                .value
                .get("from_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: "drop event missing from_id".into(),
                    data: None,
                })?;
            let from_rest = from_id.strip_prefix("rack-row:").ok_or_else(|| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("from_id has unexpected shape {from_id}"),
                data: None,
            })?;
            let source_eid: Uuid = from_rest.parse().map_err(|_| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("bad source effect id {from_rest}"),
                data: None,
            })?;
            if source_eid == target_eid {
                return Ok(json!({}));
            }
            let mut order = state.chain_order(evt.channel_uuid).await;
            // Remove source from its current spot, then insert it at the
            // target's spot. Net effect: dropping A onto B places A right
            // before B (or right after if A used to be earlier in the list).
            let Some(src_pos) = order.iter().position(|id| *id == source_eid) else {
                return Ok(json!({}));
            };
            order.remove(src_pos);
            let target_pos = order.iter().position(|id| *id == target_eid).unwrap_or(order.len());
            order.insert(target_pos, source_eid);
            chain_ops::reorder_chain(state.clone(), evt.channel_uuid, order)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("reorder_chain: {e}"),
                    data: None,
                })?;
        }
        "bypass" => {
            let effect_id: Uuid = rest.parse().map_err(|_| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("bad effect id {rest}"),
                data: None,
            })?;
            let toggled_on = evt.value.as_bool().unwrap_or(true);
            state
                .set_effect_bypassed(evt.channel_uuid, effect_id, !toggled_on)
                .await;
        }
        "chain_bypass" => {
            let toggled_on = evt.value.as_bool().unwrap_or(true);
            state
                .set_chain_bypass(evt.channel_uuid, !toggled_on)
                .await;
        }
        "open" => {
            let effect_id: Uuid = rest.parse().map_err(|_| RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("bad effect id {rest}"),
                data: None,
            })?;
            let slot = state
                .effects
                .lock()
                .await
                .get(&(evt.channel_uuid, effect_id))
                .cloned()
                .ok_or_else(|| RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: format!("effect not found: {effect_id}"),
                    data: None,
                })?;
            let engine = state.engine().ok_or_else(|| RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: "audio engine not ready".to_string(),
                data: None,
            })?;
            let title = format!("{} — Tideline", slot.effect.display_name);
            engine
                .show_ui_for_slot(evt.channel_uuid, effect_id, &title)
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("show_ui_for_slot: {e:#}"),
                    data: None,
                })?;
            return Ok(json!({}));
        }
        "rescan" => {
            // Wipe the on-disk cache so we never silently merge stale entries.
            let _ = discovery::clear_cache();
            // Tell every format to rebuild its plugin index. For LV2
            // this rebuilds livi's `World` so plugins installed since
            // app start become visible — without this step `scan_all`
            // only re-iterates whatever lilv discovered at boot.
            if let Some(engine) = state.engine() {
                engine.formats().refresh_all();
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let fresh = discovery::scan_via_state(state);
            state.set_catalog(fresh.clone()).await;
            let cache = discovery::PluginScanCache {
                schema_version: discovery::CACHE_SCHEMA_VERSION,
                scanned_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                source_mtime_max: discovery::max_source_mtime(),
                source_entry_count: discovery::source_entry_count(),
                plugins: fresh,
            };
            let _ = discovery::save_cache(&cache);
        }
        _ => {
            return Err(RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("unknown rack event {}", evt.node_id),
                data: None,
            });
        }
    }

    let mutated = matches!(kind, "add" | "remove" | "bypass" | "chain_bypass" | "rack-row");
    if mutated {
        if let Err(e) = persist_channel(state, &host, evt.channel_uuid).await {
            tracing::warn!(
                target: "tideline-effects::rack",
                error = ?e,
                channel_uuid = %evt.channel_uuid,
                "persist_channel failed after rack event"
            );
        }
    }

    if let Err(e) = host
        .event_publish(
            RACK_CHANGED_TOPIC,
            json!({ "channel_uuid": evt.channel_uuid }),
        )
        .await
    {
        tracing::warn!(
            target: "tideline-effects::rack",
            error = ?e,
            "event_publish rack_changed failed"
        );
    }

    let any_fx = !state.channels_with_effects().await.is_empty();
    let _ = host
        .register_channel_overlay(json!({
            "surface_id": "channel_card",
            "placement": "channel_card",
            "channel_filter": { "kind": "all" },
            "tree": channel_card_tree(any_fx),
        }))
        .await;

    Ok(json!({}))
}
