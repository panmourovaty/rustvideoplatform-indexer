use serde::Deserialize;
use std::collections::HashMap;
use std::env;

#[derive(Deserialize, Clone)]
pub struct MeilisearchEmbedderConfig {
    #[serde(default = "default_meilisearch_embedder_name")]
    pub name: String,
    #[serde(default = "default_meilisearch_embedder_source")]
    pub source: String,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub revision: Option<String>,
    pub pooling: Option<String>,
    pub document_template: Option<String>,
    pub document_template_max_bytes: Option<usize>,
    pub dimensions: Option<usize>,
    pub request: Option<serde_json::Value>,
    pub response: Option<serde_json::Value>,
    pub headers: Option<HashMap<String, String>>,
    pub binary_quantized: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub scylla_nodes: Vec<String>,
    #[serde(default = "default_keyspace")]
    pub scylla_keyspace: String,
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
    /// Full Meilisearch embedder settings used to configure the media index.
    #[serde(default = "default_meilisearch_embedder")]
    pub meilisearch_embedder: MeilisearchEmbedderConfig,
    /// Batch size for initial full sync (default: 1000)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Redis/Dragonfly URL for caching trending metrics and reaction counts
    pub redis_url: String,
    /// Interval in seconds between cache refreshes (default: 60)
    #[serde(default = "default_cache_interval")]
    pub cache_interval_secs: u64,
    /// Interval in seconds between polling syncs (default: 30)
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Path to the source directory containing media files (for sprite generation)
    #[serde(default = "default_source_dir")]
    pub source_dir: String,
    /// Number of top trending items to include in the sprite (default: 30)
    #[serde(default = "default_sprite_items")]
    pub sprite_items: usize,
    /// Canonical base URL of the site used when building sitemap URLs (e.g. "https://example.com")
    pub site_url: String,
}

fn default_batch_size() -> usize {
    1000
}

fn default_meilisearch_embedder() -> MeilisearchEmbedderConfig {
    MeilisearchEmbedderConfig {
        name: default_meilisearch_embedder_name(),
        source: default_meilisearch_embedder_source(),
        url: Some("http://embedllama:11434/api/embeddings".to_string()),
        api_key: None,
        model: None,
        revision: None,
        pooling: None,
        document_template: Some("{{doc.name}} {{doc.description}}".to_string()),
        document_template_max_bytes: None,
        dimensions: Some(768),
        request: Some(serde_json::json!({
            "model": "qwen3-embedding",
            "prompt": "{{text}}"
        })),
        response: Some(serde_json::json!({
            "embedding": "{{embedding}}"
        })),
        headers: None,
        binary_quantized: None,
    }
}

fn default_meilisearch_embedder_name() -> String {
    "default".to_string()
}

fn default_meilisearch_embedder_source() -> String {
    "rest".to_string()
}

fn default_keyspace() -> String {
    "videoplatform".to_string()
}

fn default_cache_interval() -> u64 {
    60
}

fn default_poll_interval() -> u64 {
    30
}

fn default_source_dir() -> String {
    "source".to_string()
}

fn default_sprite_items() -> usize {
    30
}

impl Config {
    pub fn load() -> Self {
        // Check for config.json first, then fall back to environment variables
        if let Ok(contents) = std::fs::read_to_string("config.json") {
            return serde_json::from_str(&contents).expect("Failed to parse config.json");
        }

        Config {
            scylla_nodes: env::var("SCYLLA_NODES")
                .expect("SCYLLA_NODES must be set (or provide config.json)")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            scylla_keyspace: env::var("SCYLLA_KEYSPACE")
                .unwrap_or_else(|_| default_keyspace()),
            meilisearch_url: env::var("MEILISEARCH_URL")
                .unwrap_or_else(|_| "http://localhost:7700".to_string()),
            meilisearch_key: env::var("MEILISEARCH_KEY").ok(),
            meilisearch_embedder: MeilisearchEmbedderConfig {
                name: env::var("MEILISEARCH_EMBEDDER")
                    .unwrap_or_else(|_| default_meilisearch_embedder_name()),
                source: env::var("MEILISEARCH_EMBEDDER_SOURCE")
                    .unwrap_or_else(|_| default_meilisearch_embedder_source()),
                url: env::var("MEILISEARCH_EMBEDDER_URL")
                    .ok()
                    .or_else(|| default_meilisearch_embedder().url),
                api_key: env::var("MEILISEARCH_EMBEDDER_API_KEY").ok(),
                model: env::var("MEILISEARCH_EMBEDDER_MODEL").ok(),
                revision: env::var("MEILISEARCH_EMBEDDER_REVISION").ok(),
                pooling: env::var("MEILISEARCH_EMBEDDER_POOLING").ok(),
                document_template: env::var("MEILISEARCH_EMBEDDER_DOCUMENT_TEMPLATE")
                    .ok()
                    .or_else(|| default_meilisearch_embedder().document_template),
                document_template_max_bytes: env::var("MEILISEARCH_EMBEDDER_DOCUMENT_TEMPLATE_MAX_BYTES")
                    .ok()
                    .and_then(|v| v.parse().ok()),
                dimensions: env::var("MEILISEARCH_EMBEDDER_DIMENSIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| default_meilisearch_embedder().dimensions),
                request: env::var("MEILISEARCH_EMBEDDER_REQUEST")
                    .ok()
                    .and_then(|v| serde_json::from_str(&v).ok())
                    .or_else(|| default_meilisearch_embedder().request),
                response: env::var("MEILISEARCH_EMBEDDER_RESPONSE")
                    .ok()
                    .and_then(|v| serde_json::from_str(&v).ok())
                    .or_else(|| default_meilisearch_embedder().response),
                headers: env::var("MEILISEARCH_EMBEDDER_HEADERS")
                    .ok()
                    .and_then(|v| serde_json::from_str(&v).ok()),
                binary_quantized: env::var("MEILISEARCH_EMBEDDER_BINARY_QUANTIZED")
                    .ok()
                    .and_then(|v| v.parse().ok()),
            },
            batch_size: env::var("BATCH_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_batch_size),
            redis_url: env::var("REDIS_URL")
                .expect("REDIS_URL must be set (or provide config.json)"),
            cache_interval_secs: env::var("CACHE_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_cache_interval),
            poll_interval_secs: env::var("POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_poll_interval),
            source_dir: env::var("SOURCE_DIR")
                .unwrap_or_else(|_| default_source_dir()),
            sprite_items: env::var("SPRITE_ITEMS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_sprite_items),
            site_url: env::var("SITE_URL")
                .expect("SITE_URL must be set (or provide config.json)"),
        }
    }
}
