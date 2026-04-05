/// Cloud API fallback for protected sites.
/// Copied from webclaw-mcp/src/cloud.rs to avoid coupling the crates.
use std::time::Duration;

use serde_json::{Value, json};
use tracing::info;

const API_BASE: &str = "https://api.webclaw.io/v1";

pub struct CloudClient {
    api_key: String,
    http: reqwest::Client,
}

impl CloudClient {
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("WEBCLAW_API_KEY").ok()?;
        if key.is_empty() {
            return None;
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Some(Self { api_key: key, http })
    }

    pub async fn scrape(
        &self,
        url: &str,
        formats: &[&str],
        include_selectors: &[String],
        exclude_selectors: &[String],
        only_main_content: bool,
    ) -> Result<Value, String> {
        let mut body = json!({ "url": url, "formats": formats });
        if only_main_content {
            body["only_main_content"] = json!(true);
        }
        if !include_selectors.is_empty() {
            body["include_selectors"] = json!(include_selectors);
        }
        if !exclude_selectors.is_empty() {
            body["exclude_selectors"] = json!(exclude_selectors);
        }
        self.post("scrape", body).await
    }

    pub async fn post(&self, endpoint: &str, body: Value) -> Result<Value, String> {
        let resp = self
            .http
            .post(format!("{API_BASE}/{endpoint}"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Cloud API request failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let truncated = if text.len() > 500 { &text[..500] } else { &text };
            return Err(format!("Cloud API error {status}: {truncated}"));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Cloud API response parse failed: {e}"))
    }

    pub async fn get(&self, endpoint: &str) -> Result<Value, String> {
        let resp = self
            .http
            .get(format!("{API_BASE}/{endpoint}"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| format!("Cloud API request failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let truncated = if text.len() > 500 { &text[..500] } else { &text };
            return Err(format!("Cloud API error {status}: {truncated}"));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Cloud API response parse failed: {e}"))
    }
}

pub fn is_bot_protected(html: &str, headers: &webclaw_fetch::HeaderMap) -> bool {
    let html_lower = html.to_lowercase();

    if html_lower.contains("_cf_chl_opt") || html_lower.contains("challenge-platform") {
        return true;
    }
    if (html_lower.contains("just a moment") || html_lower.contains("checking your browser"))
        && html_lower.contains("cf-spinner")
    {
        return true;
    }
    if (html_lower.contains("cf-turnstile")
        || html_lower.contains("challenges.cloudflare.com/turnstile"))
        && html.len() < 100_000
    {
        return true;
    }
    if html_lower.contains("geo.captcha-delivery.com")
        || html_lower.contains("captcha-delivery.com/captcha")
    {
        return true;
    }
    if html_lower.contains("awswaf-captcha") || html_lower.contains("aws-waf-client-browser") {
        return true;
    }
    if html_lower.contains("hcaptcha.com")
        && html_lower.contains("h-captcha")
        && html.len() < 50_000
    {
        return true;
    }
    let has_cf_headers = headers.get("cf-ray").is_some() || headers.get("cf-mitigated").is_some();
    if has_cf_headers
        && (html_lower.contains("just a moment") || html_lower.contains("checking your browser"))
    {
        return true;
    }
    false
}

pub fn needs_js_rendering(word_count: usize, html: &str) -> bool {
    let has_scripts = html.contains("<script");

    if word_count < 50 && html.len() > 5_000 && has_scripts {
        return true;
    }

    if word_count < 800 && html.len() > 50_000 && has_scripts {
        let html_lower = html.to_lowercase();
        let has_spa_marker = html_lower.contains("react-app")
            || html_lower.contains("id=\"__next\"")
            || html_lower.contains("id=\"root\"")
            || html_lower.contains("id=\"app\"")
            || html_lower.contains("__next_data__")
            || html_lower.contains("nuxt")
            || html_lower.contains("ng-app");

        if has_spa_marker {
            return true;
        }
    }
    false
}

pub enum SmartFetchResult {
    Local(Box<webclaw_core::ExtractionResult>),
    Cloud(Value),
}

pub async fn smart_fetch(
    client: &webclaw_fetch::FetchClient,
    cloud: Option<&CloudClient>,
    url: &str,
    include_selectors: &[String],
    exclude_selectors: &[String],
    only_main_content: bool,
    formats: &[&str],
) -> Result<SmartFetchResult, String> {
    let fetch_result = tokio::time::timeout(Duration::from_secs(30), client.fetch(url))
        .await
        .map_err(|_| format!("Fetch timed out after 30s for {url}"))?
        .map_err(|e| format!("Fetch failed: {e}"))?;

    if is_bot_protected(&fetch_result.html, &fetch_result.headers) {
        info!(url, "bot protection detected, falling back to cloud API");
        return cloud_fallback(cloud, url, include_selectors, exclude_selectors, only_main_content, formats).await;
    }

    let options = webclaw_core::ExtractionOptions {
        include_selectors: include_selectors.to_vec(),
        exclude_selectors: exclude_selectors.to_vec(),
        only_main_content,
        include_raw_html: false,
    };

    let extraction =
        webclaw_core::extract_with_options(&fetch_result.html, Some(&fetch_result.url), &options)
            .map_err(|e| format!("Extraction failed: {e}"))?;

    if needs_js_rendering(extraction.metadata.word_count, &fetch_result.html) {
        info!(url, word_count = extraction.metadata.word_count, "JS-rendered page detected, falling back to cloud API");
        return cloud_fallback(cloud, url, include_selectors, exclude_selectors, only_main_content, formats).await;
    }

    Ok(SmartFetchResult::Local(Box::new(extraction)))
}

async fn cloud_fallback(
    cloud: Option<&CloudClient>,
    url: &str,
    include_selectors: &[String],
    exclude_selectors: &[String],
    only_main_content: bool,
    formats: &[&str],
) -> Result<SmartFetchResult, String> {
    match cloud {
        Some(c) => {
            let resp = c.scrape(url, formats, include_selectors, exclude_selectors, only_main_content).await?;
            info!(url, "cloud API fallback successful");
            Ok(SmartFetchResult::Cloud(resp))
        }
        None => Err(format!(
            "Bot protection detected on {url}. Set WEBCLAW_API_KEY for automatic cloud bypass."
        )),
    }
}
