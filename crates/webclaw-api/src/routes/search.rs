use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::debug;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub num_results: Option<usize>,
    pub scrape: Option<bool>,
    pub formats: Option<Vec<String>>,
}

/// Search result parsed from Google HTML.
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.query.is_empty() {
        return Err(ApiError::BadRequest("query must not be empty".into()));
    }

    let num = req.num_results.unwrap_or(5).min(20);
    let should_scrape = req.scrape.unwrap_or(true);

    // Fetch Google search results
    let encoded_query = urlencoding::encode(&req.query);
    let search_url = format!("https://www.google.com/search?q={encoded_query}&num={num}");

    let fetch_result = state
        .fetch_client
        .fetch(&search_url)
        .await
        .map_err(|e| ApiError::Internal(format!("Google search fetch failed: {e}")))?;

    let results = parse_google_results(&fetch_result.html, num);
    debug!(count = results.len(), "parsed Google search results");

    if results.is_empty() {
        return Ok(Json(json!({
            "success": true,
            "query": req.query,
            "results": [],
        })));
    }

    // Optionally scrape each result URL
    if should_scrape && !results.is_empty() {
        let urls: Vec<&str> = results.iter().map(|r| r.url.as_str()).collect();
        let format = req
            .formats
            .as_ref()
            .and_then(|f| f.first())
            .map(String::as_str)
            .unwrap_or("markdown");

        let extractions = state.fetch_client.fetch_and_extract_batch(&urls, 5).await;

        let data: Vec<Value> = results
            .iter()
            .zip(extractions.iter())
            .map(|(sr, ext)| {
                let mut entry = json!({
                    "title": sr.title,
                    "url": sr.url,
                    "snippet": sr.snippet,
                });
                match &ext.result {
                    Ok(extraction) => {
                        let content = match format {
                            "llm" => webclaw_core::to_llm_text(extraction, Some(&sr.url)),
                            "text" => extraction.content.plain_text.clone(),
                            _ => extraction.content.markdown.clone(),
                        };
                        entry["content"] = json!(content);
                        entry["metadata"] =
                            serde_json::to_value(&extraction.metadata).unwrap_or_default();
                    }
                    Err(e) => {
                        entry["scrape_error"] = json!(e.to_string());
                    }
                }
                entry
            })
            .collect();

        return Ok(Json(json!({
            "success": true,
            "query": req.query,
            "results": data,
        })));
    }

    // No scrape — return search results only
    let data: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
            })
        })
        .collect();

    Ok(Json(json!({
        "success": true,
        "query": req.query,
        "results": data,
    })))
}

/// Parse Google search result HTML into structured results.
fn parse_google_results(html: &str, max: usize) -> Vec<SearchResult> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let mut results = Vec::new();

    // Google wraps each result in a div with class "g"
    let result_sel = Selector::parse("div.g").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let title_sel = Selector::parse("h3").unwrap();
    let snippet_sel = Selector::parse("div.VwiC3b, span.aCOpRe, div[data-sncf]").unwrap();

    for element in doc.select(&result_sel) {
        if results.len() >= max {
            break;
        }

        let link = match element.select(&link_sel).next() {
            Some(a) => {
                let href = a.value().attr("href").unwrap_or("");
                if href.starts_with("http") {
                    href.to_string()
                } else {
                    continue;
                }
            }
            None => continue,
        };

        let title = element
            .select(&title_sel)
            .next()
            .map(|h| h.text().collect::<String>())
            .unwrap_or_default();

        if title.is_empty() {
            continue;
        }

        let snippet = element
            .select(&snippet_sel)
            .next()
            .map(|s| s.text().collect::<String>())
            .unwrap_or_default();

        results.push(SearchResult {
            title,
            url: link,
            snippet,
        });
    }

    results
}

/// Minimal URL encoding.
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                b' ' => result.push('+'),
                _ => {
                    result.push('%');
                    result.push(char::from(HEX[(b >> 4) as usize]));
                    result.push(char::from(HEX[(b & 0x0f) as usize]));
                }
            }
        }
        result
    }

    const HEX: [u8; 16] = *b"0123456789ABCDEF";
}
