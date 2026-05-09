use crate::manifest::{self, ManifestError};
use crate::paths::plugin_install_dir;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tideline_sdk::types::Manifest;
use tideline_sdk::Capability;

const MANIFEST_FILENAME: &str = "tideline-plugin.toml";

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest: {0}")]
    Manifest(#[from] ManifestError),
    #[error("granted capability {0:?} is not in declared (required + optional)")]
    GrantedNotDeclared(Capability),
    #[error("required capability {0:?} is missing from grant")]
    RequiredNotGranted(Capability),
}

#[derive(Debug)]
pub struct InstallPreview {
    pub manifest: Manifest,
    pub source: PathBuf,
    pub declared_required: Vec<Capability>,
    pub declared_optional: Vec<Capability>,
}

pub fn inspect(source: &Path) -> Result<InstallPreview, InstallError> {
    let manifest_path = if source.is_dir() {
        source.join(MANIFEST_FILENAME)
    } else {
        return Err(InstallError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "zip install not implemented in wave 1; pass a directory",
        )));
    };
    let manifest = manifest::load(&manifest_path)?;
    Ok(InstallPreview {
        declared_required: manifest.capabilities.required.clone(),
        declared_optional: manifest.capabilities.optional.clone(),
        manifest,
        source: source.to_path_buf(),
    })
}

pub fn commit_install(
    preview: &InstallPreview,
    granted: &[Capability],
) -> Result<PathBuf, InstallError> {
    let declared: std::collections::HashSet<Capability> = preview
        .declared_required
        .iter()
        .copied()
        .chain(preview.declared_optional.iter().copied())
        .collect();
    for g in granted {
        if !declared.contains(g) {
            return Err(InstallError::GrantedNotDeclared(*g));
        }
    }
    for r in &preview.declared_required {
        if !granted.contains(r) {
            return Err(InstallError::RequiredNotGranted(*r));
        }
    }

    let dest = plugin_install_dir(&preview.manifest.plugin.id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    copy_dir(&preview.source, &dest)?;
    Ok(dest)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> PathBuf {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        PathBuf::from(manifest).join("tests/fixtures/tideline-test-plugin")
    }

    #[test]
    fn inspect_fixture_succeeds() {
        let preview = inspect(&fixture_root()).unwrap();
        assert_eq!(preview.manifest.plugin.id, "io.tideline.test");
        assert!(!preview.declared_required.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn commit_rejects_undeclared_grant() {
        let preview = inspect(&fixture_root()).unwrap();
        let mut granted = preview.declared_required.clone();
        granted.push(Capability::FsWrite);
        std::env::set_var("XDG_DATA_HOME", tempfile::tempdir().unwrap().path());
        let err = commit_install(&preview, &granted).unwrap_err();
        assert!(matches!(err, InstallError::GrantedNotDeclared(_)));
    }

    #[test]
    #[serial_test::serial]
    fn commit_rejects_missing_required() {
        let preview = inspect(&fixture_root()).unwrap();
        let granted: Vec<Capability> = vec![];
        std::env::set_var("XDG_DATA_HOME", tempfile::tempdir().unwrap().path());
        let err = commit_install(&preview, &granted).unwrap_err();
        assert!(matches!(err, InstallError::RequiredNotGranted(_)));
    }

    #[test]
    #[serial_test::serial]
    fn commit_succeeds_and_copies_manifest() {
        let preview = inspect(&fixture_root()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", dir.path());
        let granted = preview.declared_required.clone();
        let installed = commit_install(&preview, &granted).unwrap();
        assert!(installed.join(MANIFEST_FILENAME).exists());
    }
}
