use tideline_sdk::types::Manifest;

const SAMPLE: &str = r#"
[plugin]
schema = 1
id = "io.tideline.test"
name = "Test Plugin"
version = "0.0.1"
publisher = "Tideline"

[host]
api = "1.x"

[entry]
exec = "tideline-test-plugin"

[capabilities]
required = ["channel.read", "events.publish"]
optional = ["audio.play"]

[contributes]
publishes_topics = ["io.tideline.test:smoketest_done"]

[[contributes.channel_overlays]]
slot = "footer"
icon = "icon.png"
"#;

#[test]
fn parses_full_manifest() {
    let m: Manifest = toml::from_str(SAMPLE).unwrap();
    assert_eq!(m.plugin.id, "io.tideline.test");
    assert_eq!(m.plugin.schema, 1);
    assert_eq!(m.host.api, "1.x");
    assert_eq!(m.entry.exec, "tideline-test-plugin");
    assert_eq!(m.capabilities.required.len(), 2);
    assert_eq!(m.capabilities.optional.len(), 1);
    assert_eq!(m.contributes.publishes_topics, vec!["io.tideline.test:smoketest_done"]);
    assert_eq!(m.contributes.channel_overlays[0].slot, "footer");
}

#[test]
fn rejects_missing_required_section() {
    let bad = "[plugin]\nschema = 1\nid = \"x\"\nname = \"x\"\nversion = \"0.1.0\"\npublisher = \"x\"\n";
    let err = toml::from_str::<Manifest>(bad);
    assert!(err.is_err(), "manifest without [host] should fail");
}
