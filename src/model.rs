use serde::{Deserialize, Serialize};

/// Document schema matching MEILISEARCH_SCHEMA.md from rustvideoplatform.
/// Each document represents a media item in the Meilisearch "media" index.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeiliMedia {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub views: i64,
    pub likes: i64,
    pub dislikes: i64,
    #[sqlx(rename = "type")]
    pub r#type: String,
    pub upload: i64,
    pub public: bool,
    pub visibility: String,
    pub restricted_to_group: Option<String>,
}

/// Payload received via PostgreSQL NOTIFY for media changes.
#[derive(Debug, Deserialize)]
pub struct MediaChangeEvent {
    pub operation: String, // INSERT, UPDATE, DELETE
    pub id: String,
}
