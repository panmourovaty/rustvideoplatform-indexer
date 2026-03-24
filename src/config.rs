use serde::Deserialize;
use std::env;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub scylla_nodes: Vec<String>,
    #[serde(default = "default_keyspace")]
    pub scylla_keyspace: String,
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
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
    /// URL of the llama.cpp server embeddings endpoint used for vector similarity search.
    /// When set the indexer configures a REST embedder in the Meilisearch media index so
    /// that every indexed document is automatically embedded and SimilarQuery works in the
    /// main platform.
    /// Example: "http://llama-cpp:8080/v1/embeddings"
    pub llama_cpp_url: Option<String>,
    /// Name of the Meilisearch embedder to configure (default: "default").
    /// Must match the meilisearch_embedder value in the main platform config.
    #[serde(default = "default_embedder_name")]
    pub meilisearch_embedder: String,
    /// Number of embedding vector dimensions produced by the model (e.g. 768, 1024, 1536).
    /// Required when llama_cpp_url is set.
    pub embedding_dimensions: Option<usize>,
}

fn default_batch_size() -> usize {
    1000
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

fn default_embedder_name() -> String {
    "default".to_string()
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
            llama_cpp_url: env::var("LLAMA_CPP_URL").ok(),
            meilisearch_embedder: env::var("MEILISEARCH_EMBEDDER")
                .unwrap_or_else(|_| default_embedder_name()),
            embedding_dimensions: env::var("EMBEDDING_DIMENSIONS")
                .ok()
                .and_then(|v| v.parse().ok()),
        }
    }
}
