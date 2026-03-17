use log::{error, info};
use sqlx::PgPool;

use crate::meilisearch::MeiliIndex;
use crate::model::{MeiliList, MeiliMedia, MeiliUser};
use crate::sitemap;

type RedisConn = redis::aio::ConnectionManager;

// --- Media ---

/// Perform a full sync of all media records from PostgreSQL into Meilisearch.
pub async fn full_sync(
    pool: &PgPool,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full sync from PostgreSQL to Meilisearch...");

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media")
        .fetch_one(pool)
        .await?;
    let total = total.0 as u64;

    info!("Found {total} media records to index");

    if total == 0 {
        info!("No records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let batch: Vec<MeiliMedia> = sqlx::query_as(
            "SELECT m.id, m.name, m.owner, m.views, \
             COUNT(*) FILTER (WHERE ml.reaction = 'like') AS likes, \
             COUNT(*) FILTER (WHERE ml.reaction = 'dislike') AS dislikes, \
             m.type, m.upload, m.public, m.visibility, m.restricted_to_group \
             FROM media m \
             LEFT JOIN media_likes ml ON m.id = ml.media_id \
             GROUP BY m.id, m.name, m.owner, m.views, m.type, m.upload, m.public, m.visibility, m.restricted_to_group \
             ORDER BY m.upload ASC LIMIT $1 OFFSET $2",
        )
        .bind(batch_size as i64)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        if batch.is_empty() {
            break;
        }

        let count = batch.len() as u64;
        meili.add_documents(&batch).await?;
        indexed += count;
        offset += batch_size as i64;

        info!("Indexed {indexed}/{total} documents");

        if count < batch_size as u64 {
            break;
        }
    }

    info!("Full sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single media record by ID and upsert it into Meilisearch.
pub async fn sync_single(
    pool: &PgPool,
    meili: &MeiliIndex,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let row: Option<MeiliMedia> = sqlx::query_as(
        "SELECT m.id, m.name, m.owner, m.views, \
         COUNT(*) FILTER (WHERE ml.reaction = 'like') AS likes, \
         COUNT(*) FILTER (WHERE ml.reaction = 'dislike') AS dislikes, \
         m.type, m.upload, m.public, m.visibility, m.restricted_to_group \
         FROM media m \
         LEFT JOIN media_likes ml ON m.id = ml.media_id \
         WHERE m.id = $1 \
         GROUP BY m.id, m.name, m.owner, m.views, m.type, m.upload, m.public, m.visibility, m.restricted_to_group",
    )
    .bind(media_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(doc) => {
            meili.upsert_document(&doc).await?;
            info!("Upserted document '{media_id}' in Meilisearch");
        }
        None => {
            meili.delete_document(media_id).await?;
            info!("Deleted document '{media_id}' from Meilisearch (not found in DB)");
        }
    }

    Ok(())
}

/// Handle a media change event received via LISTEN/NOTIFY.
/// Regenerates the sitemap after any media change.
pub async fn handle_change(
    pool: &PgPool,
    meili: &MeiliIndex,
    redis: &mut RedisConn,
    base_url: &str,
    operation: &str,
    media_id: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(media_id).await,
        "INSERT" | "UPDATE" => sync_single(pool, meili, media_id).await,
        other => {
            error!("Unknown operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for '{media_id}': {e}");
    }

    if let Err(e) = sitemap::generate_and_store(pool, redis, base_url).await {
        error!("Failed to regenerate sitemap after media change: {e}");
    }
}

// --- Lists ---

/// Perform a full sync of all public/restricted lists from PostgreSQL into Meilisearch.
pub async fn full_sync_lists(
    pool: &PgPool,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full list sync from PostgreSQL to Meilisearch...");

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM lists WHERE visibility != 'hidden'")
        .fetch_one(pool)
        .await?;
    let total = total.0 as u64;

    info!("Found {total} list records to index");

    if total == 0 {
        info!("No list records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let batch: Vec<MeiliList> = sqlx::query_as(
            "SELECT l.id, l.name, l.owner, l.visibility, l.restricted_to_group, \
             COALESCE((SELECT COUNT(*) FROM list_items li WHERE li.list_id = l.id), 0)::bigint AS item_count, \
             l.created \
             FROM lists l \
             WHERE l.visibility != 'hidden' \
             ORDER BY l.created ASC LIMIT $1 OFFSET $2",
        )
        .bind(batch_size as i64)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        if batch.is_empty() {
            break;
        }

        let count = batch.len() as u64;
        meili.add_documents(&batch).await?;
        indexed += count;
        offset += batch_size as i64;

        info!("Indexed {indexed}/{total} list documents");

        if count < batch_size as u64 {
            break;
        }
    }

    info!("Full list sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single list record by ID and upsert/delete it in Meilisearch.
pub async fn sync_single_list(
    pool: &PgPool,
    meili: &MeiliIndex,
    list_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let row: Option<MeiliList> = sqlx::query_as(
        "SELECT l.id, l.name, l.owner, l.visibility, l.restricted_to_group, \
         COALESCE((SELECT COUNT(*) FROM list_items li WHERE li.list_id = l.id), 0)::bigint AS item_count, \
         l.created \
         FROM lists l WHERE l.id = $1",
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(doc) if doc.visibility != "hidden" => {
            meili.upsert_document(&doc).await?;
            info!("Upserted list '{list_id}' in Meilisearch");
        }
        Some(_) => {
            // Hidden lists are not searchable — remove from index if present
            meili.delete_document(list_id).await?;
            info!("Removed hidden list '{list_id}' from Meilisearch");
        }
        None => {
            meili.delete_document(list_id).await?;
            info!("Deleted list '{list_id}' from Meilisearch (not found in DB)");
        }
    }

    Ok(())
}

/// Handle a list change event received via LISTEN/NOTIFY.
/// Regenerates the sitemap after any list change.
pub async fn handle_list_change(
    pool: &PgPool,
    meili: &MeiliIndex,
    redis: &mut RedisConn,
    base_url: &str,
    operation: &str,
    list_id: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(list_id).await,
        "INSERT" | "UPDATE" => sync_single_list(pool, meili, list_id).await,
        other => {
            error!("Unknown list operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for list '{list_id}': {e}");
    }

    if let Err(e) = sitemap::generate_and_store(pool, redis, base_url).await {
        error!("Failed to regenerate sitemap after list change: {e}");
    }
}

// --- Users ---

/// Perform a full sync of all users from PostgreSQL into Meilisearch.
pub async fn full_sync_users(
    pool: &PgPool,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full user sync from PostgreSQL to Meilisearch...");

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    let total = total.0 as u64;

    info!("Found {total} user records to index");

    if total == 0 {
        info!("No user records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let batch: Vec<MeiliUser> = sqlx::query_as(
            "SELECT login, name, profile_picture FROM users ORDER BY login ASC LIMIT $1 OFFSET $2",
        )
        .bind(batch_size as i64)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        if batch.is_empty() {
            break;
        }

        let count = batch.len() as u64;
        meili.add_documents(&batch).await?;
        indexed += count;
        offset += batch_size as i64;

        info!("Indexed {indexed}/{total} user documents");

        if count < batch_size as u64 {
            break;
        }
    }

    info!("Full user sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single user record by login and upsert/delete it in Meilisearch.
pub async fn sync_single_user(
    pool: &PgPool,
    meili: &MeiliIndex,
    login: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let row: Option<MeiliUser> = sqlx::query_as(
        "SELECT login, name, profile_picture FROM users WHERE login = $1",
    )
    .bind(login)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(doc) => {
            meili.upsert_document(&doc).await?;
            info!("Upserted user '{login}' in Meilisearch");
        }
        None => {
            meili.delete_document(login).await?;
            info!("Deleted user '{login}' from Meilisearch (not found in DB)");
        }
    }

    Ok(())
}

/// Handle a user change event received via LISTEN/NOTIFY.
pub async fn handle_user_change(
    pool: &PgPool,
    meili: &MeiliIndex,
    operation: &str,
    login: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(login).await,
        "INSERT" | "UPDATE" => sync_single_user(pool, meili, login).await,
        other => {
            error!("Unknown user operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for user '{login}': {e}");
    }
}
