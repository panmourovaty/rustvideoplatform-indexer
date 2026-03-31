use serde::{Deserialize, Serialize};

/// Document schema for the Meilisearch "media" index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeiliMedia {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub views: i64,
    pub likes: i64,
    pub dislikes: i64,
    pub r#type: String,
    pub upload: i64,
    pub public: bool,
    pub visibility: String,
    pub restricted_to_group: Option<String>,
    /// English subtitle text (or first available language) used to enrich embeddings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Thumbnail path relative to source_dir (e.g. `"{id}/thumbnail.avif"`).
    /// Present for all media types when a thumbnail file exists on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// Preview sprite path relative to source_dir (e.g. `"{id}/preview-sprite.avif"`).
    /// Present for videos when a preview sprite file exists on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_sprite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _vectors: Option<serde_json::Value>,
}

/// Document schema for the Meilisearch "lists" index.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeiliUser {
    /// Primary key — the user's login name.
    pub login: String,
    pub name: String,
    pub profile_picture: Option<String>,
}
