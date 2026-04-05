use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cloud;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BrandRequest {
    pub url: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BrandRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    let fetch_result = tokio::time::timeout(Duration::from_secs(30), state.fetch_client.fetch(&req.url))
        .await
        .map_err(|_| ApiError::Internal(format!("Fetch timed out for {}", req.url)))?
        .map_err(|e| ApiError::Internal(format!("Fetch failed: {e}")))?;

    if cloud::is_bot_protected(&fetch_result.html, &fetch_result.headers) {
        if let Some(ref c) = state.cloud {
            let resp = c.post("brand", json!({"url": req.url})).await.map_err(ApiError::Internal)?;
            return Ok(Json(json!({ "success": true, "data": resp })));
        }
        return Err(ApiError::Internal(format!("Bot protection detected on {}", req.url)));
    }

    let identity = webclaw_core::brand::extract_brand(&fetch_result.html, Some(&fetch_result.url));
    Ok(Json(json!({ "success": true, "data": identity })))
}
