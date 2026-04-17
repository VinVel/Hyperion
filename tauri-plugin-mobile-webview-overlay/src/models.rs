use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOverlayWebviewRequest {
    pub url: String,
    pub title: Option<String>,
    pub user_agent: Option<String>,
}
