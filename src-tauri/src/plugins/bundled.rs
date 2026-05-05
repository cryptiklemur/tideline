use std::path::Path;

#[cfg(feature = "bundled-plugins")]
pub struct BundledAsset {
    pub rel_path: &'static str,
    pub bytes: &'static [u8],
}

#[cfg(feature = "bundled-plugins")]
pub struct BundledPlugin {
    pub id: &'static str,
    pub exec_name: &'static str,
    pub binary: &'static [u8],
    pub manifest: &'static str,
    pub assets: &'static [BundledAsset],
}

#[cfg(feature = "bundled-plugins")]
pub const BUNDLED: &[BundledPlugin] = &[
    BundledPlugin {
        id: "tideline-tones",
        exec_name: "tideline-tones",
        binary: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/",
            env!("PROFILE_DIR"),
            "/tideline-tones"
        )),
        manifest: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/tideline-tones/tideline-plugin.toml"
        )),
        assets: &[],
    },
    BundledPlugin {
        id: "tideline-notifications",
        exec_name: "tideline-notifications",
        binary: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/",
            env!("PROFILE_DIR"),
            "/tideline-notifications"
        )),
        manifest: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/tideline-notifications/tideline-plugin.toml"
        )),
        assets: &[],
    },
    BundledPlugin {
        id: "tideline-ptt",
        exec_name: "tideline-ptt",
        binary: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/",
            env!("PROFILE_DIR"),
            "/tideline-ptt"
        )),
        manifest: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/tideline-ptt/tideline-plugin.toml"
        )),
        assets: &[],
    },
    BundledPlugin {
        id: "tideline-effects",
        exec_name: "tideline-effects",
        binary: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/",
            env!("PROFILE_DIR"),
            "/tideline-effects"
        )),
        manifest: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/tideline-effects/tideline-plugin.toml"
        )),
        assets: &[
            BundledAsset {
                rel_path: "webviews/detail/index.html",
                bytes: include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../plugins/tideline-effects/webviews/detail/index.html"
                )),
            },
            BundledAsset {
                rel_path: "webviews/detail/main.js",
                bytes: include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../plugins/tideline-effects/webviews/detail/main.js"
                )),
            },
            BundledAsset {
                rel_path: "webviews/detail/style.css",
                bytes: include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../plugins/tideline-effects/webviews/detail/style.css"
                )),
            },
        ],
    },
];

#[cfg(not(feature = "bundled-plugins"))]
pub struct BundledPlugin;

#[cfg(not(feature = "bundled-plugins"))]
pub const BUNDLED: &[BundledPlugin] = &[];

#[cfg(feature = "bundled-plugins")]
pub fn extract_to(install_root: &Path) -> std::io::Result<usize> {
    use std::os::unix::fs::PermissionsExt;
    let mut extracted = 0;
    for plugin in BUNDLED {
        let dir = install_root.join(plugin.id);
        let bin_dir = dir.join("bin");
        std::fs::create_dir_all(&bin_dir)?;

        let manifest_path = dir.join("tideline-plugin.toml");
        let bin_path = bin_dir.join(plugin.exec_name);

        let needs_write = match std::fs::read(&bin_path) {
            Ok(existing) => existing != plugin.binary,
            Err(_) => true,
        };
        if needs_write {
            std::fs::write(&bin_path, plugin.binary)?;
            std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755))?;
            std::fs::write(&manifest_path, plugin.manifest)?;
            extracted += 1;
        } else if !manifest_path.exists() {
            std::fs::write(&manifest_path, plugin.manifest)?;
        }

        for asset in plugin.assets {
            let asset_path = dir.join(asset.rel_path);
            if let Some(parent) = asset_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let asset_needs_write = match std::fs::read(&asset_path) {
                Ok(existing) => existing != asset.bytes,
                Err(_) => true,
            };
            if asset_needs_write {
                std::fs::write(&asset_path, asset.bytes)?;
            }
        }
    }
    Ok(extracted)
}

#[cfg(not(feature = "bundled-plugins"))]
pub fn extract_to(_install_root: &Path) -> std::io::Result<usize> {
    Ok(0)
}
