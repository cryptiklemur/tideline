fn main() {
    tauri_build::build();

    if std::env::var_os("CARGO_FEATURE_BUNDLED_PLUGINS").is_some() {
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        let manifest_dir = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"),
        );
        let workspace_root = manifest_dir
            .parent()
            .expect("workspace root above src-tauri")
            .to_path_buf();
        let target_dir = workspace_root.join("target").join(&profile);

        for id in [
            "tideline-tones",
            "tideline-notifications",
            "tideline-wave-xlr",
            "tideline-ptt",
            "tideline-effects",
        ] {
            let bin = target_dir.join(id);
            let manifest = workspace_root
                .join("plugins")
                .join(id)
                .join("tideline-plugin.toml");
            println!("cargo:rerun-if-changed={}", bin.display());
            println!("cargo:rerun-if-changed={}", manifest.display());
        }

        println!("cargo:rustc-env=PROFILE_DIR={profile}");
    }
}
