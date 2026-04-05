use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BatchRequest {
    pub urls: Vec<String>,
    pub format: Option<String>,
    pub concurrency: Option<usize>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.urls.is_empty() {
        return Err(ApiError::BadRequest("urls must not be empty".into()));
    }
    if req.urls.len() > 100 {
        return Err(ApiError::BadRequest("batch is limited to 100 URLs".into()));
    }
    for u in &req.urls {
        super::validate_url(u)?;
    }

    let format = req.format.as_deref().unwrap_or("markdown");
    let concurrency = req.concurrency.unwrap_or(5);
    let url_refs: Vec<&str> = req.urls.iter().map(String::as_str).collect();

    let results = state.fetch_client.fetch_and_extract_batch(&url_refs, concurrency).await;

    let data: Vec<Value> = results
        .iter()
        .map(|r| match &r.result {
            Ok(extraction) => {
                let content = match format {
                    "llm" => webclaw_core::to_llm_text(extraction, Some(&r.url)),
                    "text" => extraction.content.plain_text.clone(),
                    _ => extraction.content.markdown.clone(),
                };
                json!({
                    "url": r.url,
                    "success": true,
                    "content": content,
                    "metadata": serde_json::to_value(&extraction.metadata).unwrap_or_default(),
                })
            }
            Err(e) => json!({
                "url": r.url,
                "success": false,
                "error": e.to_string(),
            }),
        })
        .collect();

    Ok(Json(json!({ "success": true, "data": data })))
}
