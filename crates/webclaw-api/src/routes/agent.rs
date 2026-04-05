use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let cloud = state.cloud.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "agent-scrape requires WEBCLAW_API_KEY. Get a key at https://webclaw.io".into(),
        )
    })?;

    let resp = cloud
        .post("agent-scrape", body)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(json!({ "success": true, "data": resp })))
}
