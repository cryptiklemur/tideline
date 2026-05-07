use serde_json::json;
use tideline_core::pipewire::directive::{LoadModuleHeader, PipewireDirective};
use tideline_sdk::contribute::{
    PipewireContributeRequest, PipewireContributeResponse, SerializedAppConfig, on_pipewire_contribute,
};

#[test]
fn helper_invokes_user_closure() {
    let req = PipewireContributeRequest {
        config: SerializedAppConfig { json: json!({"channels": []}) },
        mix_mutes: vec![],
    };
    let resp: PipewireContributeResponse = on_pipewire_contribute(req, |_cfg, _mutes| {
        Ok(vec![PipewireDirective::LoadModule {
            header: LoadModuleHeader::Named("test".into()),
            args: vec![],
            rewireable_tag: None,
        }])
    })
    .unwrap();
    assert_eq!(resp.directives.len(), 1);
}
