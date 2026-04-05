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

    let encoded_query = urlencoding::encode(&req.query);

    // Startpage proxies Google results without blocking
    let search_url = format!("https://www.startpage.com/do/dsearch?query={encoded_query}&cat=web");
    let html = state
        .fetch_client
        .fetch(&search_url)
        .await
        .map_err(|e| ApiError::Internal(format!("Search failed: {e}")))?;

    let mut results = parse_startpage_results(&html.html, num);

    // Fallback to DuckDuckGo if Startpage returns nothing
    if results.is_empty() {
        debug!("Startpage returned no results, falling back to DuckDuckGo");
        let ddg_html = state
            .fetch_client
            .fetch(&format!("https://html.duckduckgo.com/html/?q={encoded_query}"))
            .await
            .map_err(|e| ApiError::Internal(format!("Search failed: {e}")))?;
        results = parse_ddg_results(&ddg_html.html, num);
    }

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

/// Parse Startpage search results (powered by Google).
fn parse_startpage_results(html: &str, max: usize) -> Vec<SearchResult> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let mut results = Vec::new();

    let result_sel = Selector::parse("div.result, div.w-gl__result").unwrap();
    let link_sel = Selector::parse("a.result-title, a.result-link, a.w-gl__result-title, a[href]").unwrap();
    let title_sel = Selector::parse("h2, h3, a.result-title, a.result-link").unwrap();
    let snippet_sel = Selector::parse("p.description, div.description, p.w-gl__description").unwrap();

    for element in doc.select(&result_sel) {
        if results.len() >= max {
            break;
        }

        let link = match element.select(&link_sel).next() {
            Some(a) => {
                let href = a.value().attr("href").unwrap_or("");
                if href.starts_with("http") && !href.contains("startpage.com") {
                    href.to_string()
                } else {
                    continue;
                }
            }
            None => continue,
        };

        let raw_title = element
            .select(&title_sel)
            .next()
            .map(|h| h.text().collect::<String>())
            .unwrap_or_default();

        // Strip inline CSS noise from Startpage titles (e.g. ".css-xxx{...}Title")
        let title = if let Some(pos) = raw_title.rfind('}') {
            raw_title[pos + 1..].trim().to_string()
        } else {
            raw_title.trim().to_string()
        };

        if title.is_empty() {
            continue;
        }

        let snippet = element
            .select(&snippet_sel)
            .next()
            .map(|s| s.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

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
