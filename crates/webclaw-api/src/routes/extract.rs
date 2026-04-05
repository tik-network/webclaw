use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cloud::{self, SmartFetchResult};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ExtractRequest {
    pub url: String,
    pub prompt: Option<String>,
    pub schema: Option<Value>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ExtractRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    if req.schema.is_none() && req.prompt.is_none() {
        return Err(ApiError::BadRequest("Either 'schema' or 'prompt' is required".into()));
    }

    // No local LLM — fall back to cloud
    if state.llm_chain.is_none() {
        let cloud = state.cloud.as_ref().ok_or_else(|| {
            ApiError::ServiceUnavailable(
                "No LLM providers available. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or WEBCLAW_API_KEY.".into(),
            )
        })?;
        let mut body = json!({"url": req.url});
        if let Some(ref schema) = req.schema {
            body["schema"] = json!(schema);
        }
        if let Some(ref prompt) = req.prompt {
            body["prompt"] = json!(prompt);
        }
        let resp = cloud.post("extract", body).await.map_err(ApiError::Internal)?;
        return Ok(Json(json!({ "success": true, "data": resp })));
    }

    let chain = state.llm_chain.as_ref().unwrap();

    let llm_content = match cloud::smart_fetch(&state.fetch_client, state.cloud.as_ref(), &req.url, &[], &[], false, &["llm", "markdown"])
        .await
        .map_err(ApiError::Internal)?
    {
        SmartFetchResult::Local(extraction) => webclaw_core::to_llm_text(&extraction, Some(&req.url)),
        SmartFetchResult::Cloud(resp) => resp
            .get("llm")
            .or_else(|| resp.get("markdown"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };

    let data = if let Some(ref schema) = req.schema {
        webclaw_llm::extract::extract_json(&llm_content, schema, chain, None)
            .await
            .map_err(|e| ApiError::Internal(format!("LLM extraction failed: {e}")))?
    } else {
        let prompt = req.prompt.as_deref().unwrap();
        webclaw_llm::extract::extract_with_prompt(&llm_content, prompt, chain, None)
            .await
            .map_err(|e| ApiError::Internal(format!("LLM extraction failed: {e}")))?
    };

    Ok(Json(json!({ "success": true, "data": data })))
}
