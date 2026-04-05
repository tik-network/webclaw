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

    let num = req.num_results.unwrap_or(5).min(25);
    let should_scrape = req.scrape.unwrap_or(true);

    // Try Google first, fall back to DuckDuckGo if blocked
    let encoded_query = urlencoding::encode(&req.query);
    let results = match google_search(&encoded_query, num).await {
        Ok(r) if !r.is_empty() => {
            debug!(count = r.len(), "Google search results");
            r
        }
        _ => {
            debug!("Google blocked, falling back to DuckDuckGo");
            let html = state
                .fetch_client
                .fetch(&format!("https://html.duckduckgo.com/html/?q={encoded_query}"))
                .await
                .map_err(|e| ApiError::Internal(format!("Search failed: {e}")))?;
            parse_ddg_results(&html.html, num)
        }
    };

    debug!(count = results.len(), "search results");

    if results.is_empty() {
        return Ok(Json(json!({
            "success": true,
            "query": req.query,
            "results": [],
        })));
    }

    if should_scrape {
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

/// Try Google search with plain reqwest (native-tls).
async fn google_search(encoded_query: &str, num: usize) -> Result<Vec<SearchResult>, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    let resp = client
        .get(format!("https://www.google.com/search?q={encoded_query}&num={num}"))
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Google returned {}", resp.status()));
    }

    let html = resp.text().await.map_err(|e| format!("body: {e}"))?;
    Ok(parse_google_results(&html, num))
}

/// Parse Google search result HTML.
fn parse_google_results(html: &str, max: usize) -> Vec<SearchResult> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let mut results = Vec::new();

    let result_sel = Selector::parse("div.g").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let title_sel = Selector::parse("h3").unwrap();
    let snippet_sel =
        Selector::parse("div.VwiC3b, span.aCOpRe, div[data-sncf], div[style*='line-clamp']")
            .unwrap();

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

/// Parse DuckDuckGo HTML search results (fallback when Google blocks).
fn parse_ddg_results(html: &str, max: usize) -> Vec<SearchResult> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let mut results = Vec::new();

    let result_sel = Selector::parse("div.result, div.web-result").unwrap();
    let link_sel = Selector::parse("a.result__a").unwrap();
    let snippet_sel = Selector::parse("a.result__snippet").unwrap();

    for element in doc.select(&result_sel) {
        if results.len() >= max {
            break;
        }

        let (title, raw_url) = match element.select(&link_sel).next() {
            Some(a) => {
                let title: String = a.text().collect();
                let href = a.value().attr("href").unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }
                (title, href)
            }
            None => continue,
        };

        let url = extract_ddg_url(&raw_url);
        if url.is_empty() || !url.starts_with("http") {
            continue;
        }

        let snippet = element
            .select(&snippet_sel)
            .next()
            .map(|s| s.text().collect::<String>())
            .unwrap_or_default();

        results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }

    results
}

/// Extract actual URL from DuckDuckGo redirect.
fn extract_ddg_url(href: &str) -> String {
    if let Some(start) = href.find("uddg=") {
        let encoded = &href[start + 5..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        url_decode(encoded)
    } else if href.starts_with("http") {
        href.to_string()
    } else {
        String::new()
    }
}

fn url_decode(s: &str) -> String {
    let mut result = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

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
