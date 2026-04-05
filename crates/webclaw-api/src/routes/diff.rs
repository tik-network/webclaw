use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cloud::{self, SmartFetchResult};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct DiffRequest {
    pub url: String,
    pub previous_snapshot: Value,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DiffRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    let previous: webclaw_core::ExtractionResult = serde_json::from_value(req.previous_snapshot)
        .map_err(|e| ApiError::BadRequest(format!("Invalid previous_snapshot: {e}")))?;

    let result = cloud::smart_fetch(&state.fetch_client, state.cloud.as_ref(), &req.url, &[], &[], false, &["markdown"])
        .await
        .map_err(ApiError::Internal)?;

    let current = match result {
        SmartFetchResult::Local(extraction) => *extraction,
        SmartFetchResult::Cloud(resp) => {
            let markdown = resp.get("markdown").and_then(|v| v.as_str()).unwrap_or("");
            if markdown.is_empty() {
                return Err(ApiError::Internal("Cloud fallback returned no content for diff".into()));
            }
            webclaw_core::ExtractionResult {
                content: webclaw_core::Content {
                    markdown: markdown.to_string(),
                    plain_text: markdown.to_string(),
                    links: Vec::new(),
                    images: Vec::new(),
                    code_blocks: Vec::new(),
                    raw_html: None,
                },
                metadata: webclaw_core::Metadata {
                    title: None,
                    description: None,
                    author: None,
                    published_date: None,
                    language: None,
                    url: Some(req.url.clone()),
                    site_name: None,
                    image: None,
                    favicon: None,
                    word_count: markdown.split_whitespace().count(),
                },
                domain_data: None,
                structured_data: Vec::new(),
            }
        }
    };

    let content_diff = webclaw_core::diff::diff(&previous, &current);
    Ok(Json(json!({ "success": true, "data": content_diff })))
}
