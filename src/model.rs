use serde::{Deserialize, Serialize};

/// Document schema for the Meilisearch "media" index.
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

/// Document schema for the Meilisearch "lists" index.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeiliList {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub visibility: String,
    pub restricted_to_group: Option<String>,
    pub item_count: i64,
    pub created: i64,
}

/// Document schema for the Meilisearch "users" index.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeiliUser {
    /// Primary key — the user's login name.
    pub login: String,
    pub name: String,
    pub profile_picture: Option<String>,
}

/// Generic change event received via PostgreSQL NOTIFY.
/// Payload JSON: `{"operation": "INSERT|UPDATE|DELETE", "id": "<id>"}`
#[derive(Debug, Deserialize)]
pub struct ChangeEvent {
    pub operation: String,
    pub id: String,
}
