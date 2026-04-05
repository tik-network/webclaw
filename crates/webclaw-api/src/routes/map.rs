use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct MapRequest {
    pub url: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MapRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    let entries = webclaw_fetch::sitemap::discover(&state.fetch_client, &req.url)
        .await
        .map_err(|e| ApiError::Internal(format!("Sitemap discovery failed: {e}")))?;

    let urls: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut entry = json!({ "url": e.url });
            if let Some(ref lastmod) = e.last_modified {
                entry["lastmod"] = json!(lastmod);
            }
            entry
        })
        .collect();

    Ok(Json(json!({ "success": true, "total": urls.len(), "urls": urls })))
}
