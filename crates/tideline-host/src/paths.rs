use std::path::PathBuf;

fn xdg(env: &str, fallback_subpath: &str) -> PathBuf {
    if let Ok(v) = std::env::var(env) {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(fallback_subpath)
}

pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("tideline")
}

pub fn config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("tideline")
}

pub fn state_home() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("tideline")
}

pub fn plugin_install_dir(plugin_id: &str) -> PathBuf {
    data_home().join("plugins").join(plugin_id)
}

pub fn plugin_log_path(plugin_id: &str) -> PathBuf {
    state_home().join("plugins").join(plugin_id).join("plugin.log")
}

pub fn plugin_permissions_path(plugin_id: &str) -> PathBuf {
    config_home().join("plugins").join(plugin_id).join("permissions.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honors_xdg_overrides() {
        std::env::set_var("XDG_DATA_HOME", "/tmp/wave1-xdg-data");
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/wave1-xdg-config");
        std::env::set_var("XDG_STATE_HOME", "/tmp/wave1-xdg-state");
        assert_eq!(plugin_install_dir("io.test"), PathBuf::from("/tmp/wave1-xdg-data/tideline/plugins/io.test"));
        assert_eq!(plugin_log_path("io.test"), PathBuf::from("/tmp/wave1-xdg-state/tideline/plugins/io.test/plugin.log"));
        assert_eq!(plugin_permissions_path("io.test"), PathBuf::from("/tmp/wave1-xdg-config/tideline/plugins/io.test/permissions.toml"));
    }
}
