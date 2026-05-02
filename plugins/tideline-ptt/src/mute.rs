use serde_json::json;
use std::time::Duration;
use tideline_sdk::transport::SdkTransportError;
use tideline_sdk::HostClient;

pub async fn set_source_mute(
    client: &HostClient,
    node: &str,
    muted: bool,
) -> Result<(), SdkTransportError> {
    client
        .call_raw(
            "host/source.set_mute",
            Some(json!({ "node": node, "muted": muted })),
            Duration::from_secs(5),
        )
        .await?;
    Ok(())
}
