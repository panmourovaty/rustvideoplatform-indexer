use serde::Deserialize;
use std::env;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub dbconnection: String,
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
    /// Batch size for initial full sync (default: 1000)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Channel name for PostgreSQL LISTEN/NOTIFY (default: "media_changes")
    #[serde(default = "default_channel")]
    pub notify_channel: String,
    /// Redis/Dragonfly URL for caching trending metrics and reaction counts
    pub redis_url: String,
    /// Interval in seconds between cache refreshes (default: 60)
    #[serde(default = "default_cache_interval")]
    pub cache_interval_secs: u64,
}

fn default_batch_size() -> usize {
    1000
}

fn default_channel() -> String {
    "media_changes".to_string()
}

fn default_cache_interval() -> u64 {
    60
}

impl Config {
    pub fn load() -> Self {
        // Check for config.json first, then fall back to environment variables
        if let Ok(contents) = std::fs::read_to_string("config.json") {
            return serde_json::from_str(&contents).expect("Failed to parse config.json");
        }

        Config {
            dbconnection: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set (or provide config.json)"),
            meilisearch_url: env::var("MEILISEARCH_URL")
                .unwrap_or_else(|_| "http://localhost:7700".to_string()),
            meilisearch_key: env::var("MEILISEARCH_KEY").ok(),
            batch_size: env::var("BATCH_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_batch_size),
            notify_channel: env::var("NOTIFY_CHANNEL")
                .unwrap_or_else(|_| default_channel()),
            redis_url: env::var("REDIS_URL")
                .expect("REDIS_URL must be set (or provide config.json)"),
            cache_interval_secs: env::var("CACHE_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_cache_interval),
        }
    }
}
