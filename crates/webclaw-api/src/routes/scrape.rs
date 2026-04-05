use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cloud::{self, SmartFetchResult};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
    pub formats: Option<Vec<String>>,
    pub include_selectors: Option<Vec<String>>,
    pub exclude_selectors: Option<Vec<String>>,
    pub only_main_content: Option<bool>,
    pub browser: Option<String>,
    pub cookies: Option<Vec<String>>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScrapeRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    let formats = req.formats.unwrap_or_else(|| vec!["markdown".into()]);
    let include = req.include_selectors.unwrap_or_default();
    let exclude = req.exclude_selectors.unwrap_or_default();
    let main_only = req.only_main_content.unwrap_or(false);

    let browser = super::parse_browser(req.browser.as_deref());
    let cookie_header = req.cookies.as_ref().filter(|c| !c.is_empty()).map(|c| c.join("; "));

    let is_default_browser = matches!(browser, webclaw_fetch::BrowserProfile::Chrome);
    let needs_custom = !is_default_browser || cookie_header.is_some();

    let custom_client;
    let client: &webclaw_fetch::FetchClient = if needs_custom {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Accept-Language".to_string(), "en-US,en;q=0.9".to_string());
        if let Some(ref cookies) = cookie_header {
            headers.insert("Cookie".to_string(), cookies.clone());
        }
        let config = webclaw_fetch::FetchConfig {
            browser,
            headers,
            ..Default::default()
        };
        custom_client = webclaw_fetch::FetchClient::new(config)
            .map_err(|e| ApiError::Internal(format!("Failed to build client: {e}")))?;
        &custom_client
    } else {
        &state.fetch_client
    };

    let format_refs: Vec<&str> = formats.iter().map(String::as_str).collect();
    let result = cloud::smart_fetch(client, state.cloud.as_ref(), &req.url, &include, &exclude, main_only, &format_refs)
        .await
        .map_err(ApiError::Internal)?;

    match result {
        SmartFetchResult::Local(extraction) => {
            let mut data = json!({});
            for fmt in &formats {
                match fmt.as_str() {
                    "markdown" => data["markdown"] = json!(extraction.content.markdown),
                    "llm" => data["llm"] = json!(webclaw_core::to_llm_text(&extraction, Some(&req.url))),
                    "text" => data["text"] = json!(extraction.content.plain_text),
                    "json" => data["json"] = serde_json::to_value(&*extraction).unwrap_or_default(),
                    "links" => data["links"] = serde_json::to_value(&extraction.content.links).unwrap_or_default(),
                    "rawHtml" => data["rawHtml"] = json!(extraction.content.raw_html),
                    _ => data[fmt.as_str()] = json!(extraction.content.markdown),
                }
            }
            data["metadata"] = serde_json::to_value(&extraction.metadata).unwrap_or_default();
            Ok(Json(json!({ "success": true, "data": data })))
        }
        SmartFetchResult::Cloud(resp) => {
            Ok(Json(json!({ "success": true, "data": resp })))
        }
    }
}
