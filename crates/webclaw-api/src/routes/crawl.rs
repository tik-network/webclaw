use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::jobs::{CrawlJob, JobStatus};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CrawlRequest {
    pub url: String,
    pub depth: Option<u32>,
    pub max_pages: Option<usize>,
    pub concurrency: Option<usize>,
    pub use_sitemap: Option<bool>,
    pub format: Option<String>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_paths: Option<Vec<String>>,
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CrawlRequest>,
) -> Result<Json<Value>, ApiError> {
    super::validate_url(&req.url)?;

    if let Some(max) = req.max_pages {
        if max > 500 {
            return Err(ApiError::BadRequest("max_pages cannot exceed 500".into()));
        }
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let _format = req.format.unwrap_or_else(|| "markdown".into());

    let config = webclaw_fetch::CrawlConfig {
        max_depth: req.depth.unwrap_or(2) as usize,
        max_pages: req.max_pages.unwrap_or(50),
        concurrency: req.concurrency.unwrap_or(5),
        use_sitemap: req.use_sitemap.unwrap_or(false),
        include_patterns: req.include_paths.unwrap_or_default(),
        exclude_patterns: req.exclude_paths.unwrap_or_default(),
        ..Default::default()
    };

    let job = CrawlJob {
        id: job_id.clone(),
        status: JobStatus::Running,
        url: req.url.clone(),
        result: None,
        error: None,
        created_at: Instant::now(),
    };
    state.jobs.insert(job);

    let url = req.url.clone();
    let jobs = Arc::clone(&state) as Arc<AppState>;
    let id = job_id.clone();

    tokio::spawn(async move {
        match webclaw_fetch::Crawler::new(&url, config) {
            Ok(crawler) => {
                let result = crawler.crawl(&url, None).await;
                jobs.jobs.update_completed(&id, result);
            }
            Err(e) => {
                jobs.jobs.update_failed(&id, format!("Crawler init failed: {e}"));
            }
        }
    });

    Ok(Json(json!({
        "success": true,
        "id": job_id,
    })))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let job = state
        .jobs
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("Crawl job {id} not found")))?;

    match job.status {
        JobStatus::Running => Ok(Json(json!({
            "success": true,
            "status": "running",
            "url": job.url,
        }))),
        JobStatus::Completed => {
            let result = job.result.as_ref().unwrap();
            let pages: Vec<Value> = result
                .pages
                .iter()
                .map(|p| {
                    let mut page = json!({
                        "url": p.url,
                        "depth": p.depth,
                    });
                    if let Some(ref extraction) = p.extraction {
                        page["markdown"] = json!(extraction.content.markdown);
                        page["metadata"] = serde_json::to_value(&extraction.metadata).unwrap_or_default();
                    }
                    if let Some(ref err) = p.error {
                        page["error"] = json!(err);
                    }
                    page
                })
                .collect();

            Ok(Json(json!({
                "success": true,
                "status": "completed",
                "total": result.total,
                "ok": result.ok,
                "errors": result.errors,
                "elapsed_secs": result.elapsed_secs,
                "data": pages,
            })))
        }
        JobStatus::Failed => Ok(Json(json!({
            "success": false,
            "status": "failed",
            "error": job.error,
        }))),
    }
}
