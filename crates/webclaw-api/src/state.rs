use std::sync::Arc;

use tracing::{error, info, warn};

use crate::cloud::CloudClient;
use crate::jobs::JobStore;

pub struct AppState {
    pub fetch_client: Arc<webclaw_fetch::FetchClient>,
    pub llm_chain: Option<webclaw_llm::ProviderChain>,
    pub cloud: Option<CloudClient>,
    pub jobs: JobStore,
    pub api_key: Option<String>,
}

impl AppState {
    pub async fn new(api_key: Option<String>) -> Self {
        let mut config = webclaw_fetch::FetchConfig::default();

        if let Ok(proxy) = std::env::var("WEBCLAW_PROXY") {
            info!("using single proxy from WEBCLAW_PROXY");
            config.proxy = Some(proxy);
        }

        let proxy_file = std::env::var("WEBCLAW_PROXY_FILE")
            .ok()
            .unwrap_or_else(|| "proxies.txt".to_string());
        if std::path::Path::new(&proxy_file).exists() {
            if let Ok(pool) = webclaw_fetch::parse_proxy_file(&proxy_file) {
                if !pool.is_empty() {
                    info!(count = pool.len(), file = %proxy_file, "loaded proxy pool");
                    config.proxy_pool = pool;
                }
            }
        }

        let fetch_client = match webclaw_fetch::FetchClient::new(config) {
            Ok(client) => client,
            Err(e) => {
                error!("failed to build FetchClient: {e}");
                std::process::exit(1);
            }
        };

        let chain = webclaw_llm::ProviderChain::default().await;
        let llm_chain = if chain.is_empty() {
            warn!("no LLM providers available -- extract/summarize endpoints will require cloud fallback");
            None
        } else {
            info!(providers = chain.len(), "LLM provider chain ready");
            Some(chain)
        };

        let cloud = CloudClient::from_env();
        if cloud.is_some() {
            info!("cloud API fallback enabled (WEBCLAW_API_KEY set)");
        }

        Self {
            fetch_client: Arc::new(fetch_client),
            llm_chain,
            cloud,
            jobs: JobStore::new(),
            api_key,
        }
    }
}
