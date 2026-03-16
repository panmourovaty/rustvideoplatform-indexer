use surrealdb::types::SurrealValue;
use serde::{Deserialize, Serialize};

/// Document schema for the Meilisearch "media" index.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct MeiliMedia {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub likes: i64,
    #[serde(default)]
    pub dislikes: i64,
    #[serde(rename = "type", default)]
    pub r#type: String,
    #[serde(default)]
    pub upload: i64,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub visibility: String,
    pub restricted_to_group: Option<String>,
}

/// Document schema for the Meilisearch "lists" index.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct MeiliList {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub visibility: String,
    pub restricted_to_group: Option<String>,
    #[serde(default)]
    pub item_count: i64,
    #[serde(default)]
    pub created: i64,
}

/// Document schema for the Meilisearch "users" index.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct MeiliUser {
    /// Primary key — the user's login name.
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub name: String,
    pub profile_picture: Option<String>,
}

/// Change event from SurrealDB live query.
#[derive(Debug, Clone)]
pub enum LiveAction {
    Upsert,
    Delete,
}
