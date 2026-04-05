use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cloud::{self, SmartFetchResult};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SummarizeRequest {
    pub url: String,
    pub max_sentences: Option<usize>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SummarizeRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    if state.llm_chain.is_none() {
        let cloud = state.cloud.as_ref().ok_or_else(|| {
            ApiError::ServiceUnavailable(
                "No LLM providers available. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or WEBCLAW_API_KEY.".into(),
            )
        })?;
        let mut body = json!({"url": req.url});
        if let Some(sentences) = req.max_sentences {
            body["max_sentences"] = json!(sentences);
        }
        let resp = cloud.post("summarize", body).await.map_err(ApiError::Internal)?;
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

    let summary = webclaw_llm::summarize::summarize(&llm_content, req.max_sentences, chain, None)
        .await
        .map_err(|e| ApiError::Internal(format!("Summarization failed: {e}")))?;

    Ok(Json(json!({ "success": true, "data": { "summary": summary } })))
}
