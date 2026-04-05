pub mod agent;
pub mod batch;
pub mod brand;
pub mod crawl;
pub mod diff;
pub mod extract;
pub mod health;
pub mod map;
pub mod scrape;
pub mod search;
pub mod summarize;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::CorsLayer;
use url::Url;

use crate::auth;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let v1 = Router::new()
        .route("/v1/scrape", post(scrape::handler))
        .route("/v1/crawl", post(crawl::start))
        .route("/v1/crawl/{id}", get(crawl::status))
        .route("/v1/batch", post(batch::handler))
        .route("/v1/map", post(map::handler))
        .route("/v1/extract", post(extract::handler))
        .route("/v1/summarize", post(summarize::handler))
        .route("/v1/diff", post(diff::handler))
        .route("/v1/brand", post(brand::handler))
        .route("/v1/search", post(search::handler))
        .route("/v1/agent-scrape", post(agent::handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    Router::new()
        .route("/health", get(health::handler))
        .merge(v1)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn validate_url(url: &str) -> Result<(), ApiError> {
    if url.is_empty() {
        return Err(ApiError::BadRequest("URL must not be empty".into()));
    }
    match Url::parse(url) {
        Ok(parsed) if parsed.scheme() == "http" || parsed.scheme() == "https" => Ok(()),
        Ok(parsed) => Err(ApiError::BadRequest(format!(
            "URL scheme '{}' not allowed, must be http or https",
            parsed.scheme()
        ))),
        Err(e) => Err(ApiError::BadRequest(format!("Invalid URL: {e}"))),
    }
}

fn parse_browser(browser: Option<&str>) -> webclaw_fetch::BrowserProfile {
    match browser {
        Some("firefox") => webclaw_fetch::BrowserProfile::Firefox,
        Some("random") => webclaw_fetch::BrowserProfile::Random,
        _ => webclaw_fetch::BrowserProfile::Chrome,
    }
}
