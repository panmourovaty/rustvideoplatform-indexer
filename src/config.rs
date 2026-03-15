use serde::Deserialize;
use std::env;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub surrealdb_url: String,
    pub surrealdb_ns: String,
    pub surrealdb_db: String,
    pub surrealdb_user: String,
    pub surrealdb_pass: String,
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    pub redis_url: String,
    #[serde(default = "default_cache_interval")]
    pub cache_interval_secs: u64,
    #[serde(default = "default_source_dir")]
    pub source_dir: String,
    #[serde(default = "default_sprite_items")]
    pub sprite_items: usize,
}

fn default_batch_size() -> usize { 1000 }
fn default_cache_interval() -> u64 { 60 }
fn default_source_dir() -> String { "source".to_string() }
fn default_sprite_items() -> usize { 30 }

impl Config {
    pub fn load() -> Self {
        if let Ok(contents) = std::fs::read_to_string("config.json") {
            return serde_json::from_str(&contents).expect("Failed to parse config.json");
        }

        Config {
            surrealdb_url: env::var("SURREALDB_URL")
                .expect("SURREALDB_URL must be set (or provide config.json)"),
            surrealdb_ns: env::var("SURREALDB_NS")
                .expect("SURREALDB_NS must be set (or provide config.json)"),
            surrealdb_db: env::var("SURREALDB_DB")
                .expect("SURREALDB_DB must be set (or provide config.json)"),
            surrealdb_user: env::var("SURREALDB_USER")
                .expect("SURREALDB_USER must be set (or provide config.json)"),
            surrealdb_pass: env::var("SURREALDB_PASS")
                .expect("SURREALDB_PASS must be set (or provide config.json)"),
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
            source_dir: env::var("SOURCE_DIR").unwrap_or_else(|_| default_source_dir()),
            sprite_items: env::var("SPRITE_ITEMS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_sprite_items),
        }
    }
}
