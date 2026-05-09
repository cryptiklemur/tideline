use std::fs;
use std::path::Path;
use tideline_core::model::AppConfig;
use tideline_core::pipewire::generate_pipewire_config;

const FIXTURE_DIR: &str = "tests/fixtures";

#[test]
fn pipewire_conf_matches_golden() {
    let input_json =
        fs::read_to_string(Path::new(FIXTURE_DIR).join("baseline_input.json")).unwrap();
    let cfg: AppConfig = serde_json::from_str(&input_json).unwrap();
    let actual = generate_pipewire_config(&cfg).unwrap();
    let expected = fs::read_to_string(Path::new(FIXTURE_DIR).join("baseline.conf")).unwrap();
    pretty_assertions::assert_eq!(actual, expected);
}

#[test]
#[ignore]
fn regenerate_golden() {
    let input_json =
        fs::read_to_string(Path::new(FIXTURE_DIR).join("baseline_input.json")).unwrap();
    let cfg: AppConfig = serde_json::from_str(&input_json).unwrap();
    let actual = generate_pipewire_config(&cfg).unwrap();
    fs::write(Path::new(FIXTURE_DIR).join("baseline.conf"), actual).unwrap();
}
