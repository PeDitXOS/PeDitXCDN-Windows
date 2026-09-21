use crate::types::PanelData;
use once_cell::sync::Lazy;
use reqwest::Client;

const PANEL_BASE: &str = "https://docproir.peditxcdn.ir:8443";

static HTTP: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        // ponytail: skip TLS verification for self-signed certs on panel.
        // Upgrade when panel gets a proper CA-signed cert.
        .danger_accept_invalid_certs(true)
        .build()
        .expect("reqwest client")
});

/// Fetch panel info. Expects the panel to return JSON with relay details.
/// Adapt the endpoint path/fields to match the actual panel API.
pub async fn fetch_panel_data() -> Result<PanelData, String> {
    let url = format!("{PANEL_BASE}/api/v1/info");
    let resp = HTTP
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("panel request failed: {e}"))?;

    let data: PanelData = resp
        .json()
        .await
        .map_err(|e| format!("panel response parse failed: {e}"))?;

    Ok(data)
}
