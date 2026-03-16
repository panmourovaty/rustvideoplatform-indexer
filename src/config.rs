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
    /// Channel name for media PostgreSQL LISTEN/NOTIFY (default: "media_changes")
    #[serde(default = "default_media_channel")]
    pub notify_channel: String,
    /// Channel name for list PostgreSQL LISTEN/NOTIFY (default: "list_changes")
    #[serde(default = "default_list_channel")]
    pub list_notify_channel: String,
    /// Channel name for user PostgreSQL LISTEN/NOTIFY (default: "user_changes")
    #[serde(default = "default_user_channel")]
    pub user_notify_channel: String,
    /// Redis/Dragonfly URL for caching trending metrics and reaction counts
    pub redis_url: String,
    /// Interval in seconds between cache refreshes (default: 60)
    #[serde(default = "default_cache_interval")]
    pub cache_interval_secs: u64,
    /// Path to the source directory containing media files (for sprite generation)
    #[serde(default = "default_source_dir")]
    pub source_dir: String,
    /// Number of top trending items to include in the sprite (default: 30)
    #[serde(default = "default_sprite_items")]
    pub sprite_items: usize,
}

fn default_batch_size() -> usize {
    1000
}

fn default_media_channel() -> String {
    "media_changes".to_string()
}

fn default_list_channel() -> String {
    "list_changes".to_string()
}

fn default_user_channel() -> String {
    "user_changes".to_string()
}

fn default_cache_interval() -> u64 {
    60
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
                .unwrap_or_else(|_| default_media_channel()),
            list_notify_channel: env::var("LIST_NOTIFY_CHANNEL")
                .unwrap_or_else(|_| default_list_channel()),
            user_notify_channel: env::var("USER_NOTIFY_CHANNEL")
                .unwrap_or_else(|_| default_user_channel()),
            redis_url: env::var("REDIS_URL")
                .expect("REDIS_URL must be set (or provide config.json)"),
            cache_interval_secs: env::var("CACHE_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_cache_interval),
            source_dir: env::var("SOURCE_DIR")
                .unwrap_or_else(|_| default_source_dir()),
            sprite_items: env::var("SPRITE_ITEMS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_sprite_items),
        }
    }
}
