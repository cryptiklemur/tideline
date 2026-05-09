use std::path::Path;
use thiserror::Error;
use tideline_sdk::types::Manifest;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported schema version {0}; host supports schema 1")]
    UnsupportedSchema(u32),
    #[error("plugin id is empty")]
    EmptyId,
    #[error("entry.exec is empty")]
    EmptyExec,
    #[error("optional capability {0:?} also listed as required")]
    OptionalConflictsRequired(tideline_sdk::Capability),
    #[error("publishes_topics entry {0:?} must be prefixed with plugin id")]
    UnscopedTopic(String),
}

pub fn load(path: &Path) -> Result<Manifest, ManifestError> {
    let raw = std::fs::read_to_string(path)?;
    let m: Manifest = toml::from_str(&raw)?;
    validate(&m)?;
    Ok(m)
}

pub fn validate(m: &Manifest) -> Result<(), ManifestError> {
    if m.plugin.schema != 1 {
        return Err(ManifestError::UnsupportedSchema(m.plugin.schema));
    }
    if m.plugin.id.trim().is_empty() {
        return Err(ManifestError::EmptyId);
    }
    if m.entry.exec.trim().is_empty() {
        return Err(ManifestError::EmptyExec);
    }
    for opt in &m.capabilities.optional {
        if m.capabilities.required.contains(opt) {
            return Err(ManifestError::OptionalConflictsRequired(*opt));
        }
    }
    let prefix = format!("{}:", m.plugin.id);
    for t in &m.contributes.publishes_topics {
        if !t.starts_with(&prefix) {
            return Err(ManifestError::UnscopedTopic(t.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tideline_sdk::types::*;
    use tideline_sdk::Capability;

    fn good() -> Manifest {
        Manifest {
            plugin: ManifestPluginInfo {
                schema: 1,
                id: "io.tideline.test".into(),
                name: "Test".into(),
                version: "0.1.0".into(),
                publisher: "Tideline".into(),
            },
            host: ManifestHost { api: "1.x".into() },
            entry: ManifestEntry {
                exec: "tideline-test-plugin".into(),
            },
            capabilities: ManifestCapabilities {
                required: vec![Capability::ChannelRead],
                optional: vec![],
            },
            contributes: ManifestContributes {
                publishes_topics: vec!["io.tideline.test:done".into()],
                channel_overlays: vec![],
            },
        }
    }

    #[test]
    fn accepts_good_manifest() {
        validate(&good()).unwrap();
    }

    #[test]
    fn rejects_unscoped_topic() {
        let mut m = good();
        m.contributes.publishes_topics = vec!["other_plugin:done".into()];
        assert!(matches!(validate(&m), Err(ManifestError::UnscopedTopic(_))));
    }

    #[test]
    fn rejects_optional_overlap_required() {
        let mut m = good();
        m.capabilities.optional = vec![Capability::ChannelRead];
        assert!(matches!(
            validate(&m),
            Err(ManifestError::OptionalConflictsRequired(_))
        ));
    }

    #[test]
    fn rejects_schema_2() {
        let mut m = good();
        m.plugin.schema = 2;
        assert!(matches!(
            validate(&m),
            Err(ManifestError::UnsupportedSchema(2))
        ));
    }
}
