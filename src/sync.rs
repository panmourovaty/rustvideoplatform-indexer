use log::{error, info};
use sqlx::PgPool;

use crate::meilisearch::MeiliIndex;
use crate::model::MeiliMedia;

/// Perform a full sync of all media records from PostgreSQL into Meilisearch.
/// Documents are sent in batches to avoid memory issues with large datasets.
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
             m.type, m.upload, m.visibility, m.restricted_to_group \
             FROM media m \
             LEFT JOIN media_likes ml ON m.id = ml.media_id \
             GROUP BY m.id, m.name, m.owner, m.views, m.type, m.upload, m.visibility, m.restricted_to_group \
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
         m.type, m.upload, m.visibility, m.restricted_to_group \
         FROM media m \
         LEFT JOIN media_likes ml ON m.id = ml.media_id \
         WHERE m.id = $1 \
         GROUP BY m.id, m.name, m.owner, m.views, m.type, m.upload, m.visibility, m.restricted_to_group",
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
            // Record no longer exists in DB — remove from index
            meili.delete_document(media_id).await?;
            info!("Deleted document '{media_id}' from Meilisearch (not found in DB)");
        }
    }

    Ok(())
}

/// Handle a change event received via LISTEN/NOTIFY.
pub async fn handle_change(
    pool: &PgPool,
    meili: &MeiliIndex,
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
}
