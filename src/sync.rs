use log::{error, info};
use surrealdb::engine::remote::ws::Client as WsClient;
use surrealdb::types::{RecordId, SurrealValue};
use surrealdb::Surreal;

use crate::meilisearch::MeiliIndex;
use crate::model::{MeiliList, MeiliMedia, MeiliUser};

type Db = Surreal<WsClient>;

// --- Media ---

/// Perform a full sync of all media records from SurrealDB into Meilisearch.
pub async fn full_sync(
    db: &Db,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full sync from SurrealDB to Meilisearch...");

    let mut count_resp = db
        .query("SELECT count() FROM media GROUP ALL")
        .await?;
    #[derive(serde::Deserialize, SurrealValue)]
    struct CountRow { count: i64 }
    let count_row: Option<CountRow> = count_resp.take(0)?;
    let total = count_row.map(|r| r.count).unwrap_or(0) as u64;

    info!("Found {total} media records to index");

    if total == 0 {
        info!("No records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let mut resp = db
            .query("SELECT meta::id(id) AS id, (name ?? '') AS name, (owner ?? '') AS owner, (views ?? 0) AS views, (likes_count ?? 0) AS likes, (dislikes_count ?? 0) AS dislikes, (type ?? '') AS medium_type, (upload ?? 0) AS upload, (public ?? false) AS public, (visibility ?? '') AS visibility, restricted_to_group FROM media ORDER BY upload ASC LIMIT $batch START $offset")
            .bind(("batch", batch_size as i64))
            .bind(("offset", offset))
            .await?;

        let batch: Vec<MeiliMedia> = resp.take(0)?;

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
    db: &Db,
    meili: &MeiliIndex,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut resp = db
        .query("SELECT meta::id(id) AS id, (name ?? '') AS name, (owner ?? '') AS owner, (views ?? 0) AS views, (likes_count ?? 0) AS likes, (dislikes_count ?? 0) AS dislikes, (type ?? '') AS medium_type, (upload ?? 0) AS upload, (public ?? false) AS public, (visibility ?? '') AS visibility, restricted_to_group FROM media WHERE id = $id")
        .bind(("id", RecordId::new("media", media_id)))
        .await?;

    let row: Option<MeiliMedia> = resp.take(0)?;

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

/// Handle a media change event.
pub async fn handle_change(
    db: &Db,
    meili: &MeiliIndex,
    operation: &str,
    media_id: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(media_id).await,
        "INSERT" | "UPDATE" => sync_single(db, meili, media_id).await,
        other => {
            error!("Unknown operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for '{media_id}': {e}");
    }
}

// --- Lists ---

/// Perform a full sync of all public/restricted lists from SurrealDB into Meilisearch.
pub async fn full_sync_lists(
    db: &Db,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full list sync from SurrealDB to Meilisearch...");

    let mut count_resp = db
        .query("SELECT count() FROM lists WHERE visibility != 'hidden' GROUP ALL")
        .await?;
    #[derive(serde::Deserialize, SurrealValue)]
    struct CountRow { count: i64 }
    let count_row: Option<CountRow> = count_resp.take(0)?;
    let total = count_row.map(|r| r.count).unwrap_or(0) as u64;

    info!("Found {total} list records to index");

    if total == 0 {
        info!("No list records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let mut resp = db
            .query("SELECT meta::id(id) AS id, (name ?? '') AS name, (owner ?? '') AS owner, (visibility ?? '') AS visibility, restricted_to_group, ((SELECT count() FROM list_items WHERE list_id = $parent.id GROUP ALL)[0].count ?? 0) AS item_count, (created ?? 0) AS created FROM lists WHERE visibility != 'hidden' ORDER BY created ASC LIMIT $batch START $offset")
            .bind(("batch", batch_size as i64))
            .bind(("offset", offset))
            .await?;

        let batch: Vec<MeiliList> = resp.take(0)?;

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
    db: &Db,
    meili: &MeiliIndex,
    list_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut resp = db
        .query("SELECT meta::id(id) AS id, (name ?? '') AS name, (owner ?? '') AS owner, (visibility ?? '') AS visibility, restricted_to_group, ((SELECT count() FROM list_items WHERE list_id = $parent.id GROUP ALL)[0].count ?? 0) AS item_count, (created ?? 0) AS created FROM lists WHERE id = $id")
        .bind(("id", RecordId::new("lists", list_id)))
        .await?;

    let row: Option<MeiliList> = resp.take(0)?;

    match row {
        Some(doc) if doc.visibility != "hidden" => {
            meili.upsert_document(&doc).await?;
            info!("Upserted list '{list_id}' in Meilisearch");
        }
        Some(_) => {
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

/// Handle a list change event.
pub async fn handle_list_change(
    db: &Db,
    meili: &MeiliIndex,
    operation: &str,
    list_id: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(list_id).await,
        "INSERT" | "UPDATE" => sync_single_list(db, meili, list_id).await,
        other => {
            error!("Unknown list operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for list '{list_id}': {e}");
    }
}

// --- Users ---

/// Perform a full sync of all users from SurrealDB into Meilisearch.
pub async fn full_sync_users(
    db: &Db,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!("Starting full user sync from SurrealDB to Meilisearch...");

    let mut count_resp = db
        .query("SELECT count() FROM users GROUP ALL")
        .await?;
    #[derive(serde::Deserialize, SurrealValue)]
    struct CountRow { count: i64 }
    let count_row: Option<CountRow> = count_resp.take(0)?;
    let total = count_row.map(|r| r.count).unwrap_or(0) as u64;

    info!("Found {total} user records to index");

    if total == 0 {
        info!("No user records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    let mut offset: i64 = 0;

    loop {
        let mut resp = db
            .query("SELECT meta::id(id) AS login, (name ?? '') AS name, profile_picture FROM users ORDER BY id ASC LIMIT $batch START $offset")
            .bind(("batch", batch_size as i64))
            .bind(("offset", offset))
            .await?;

        let batch: Vec<MeiliUser> = resp.take(0)?;

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
    db: &Db,
    meili: &MeiliIndex,
    login: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut resp = db
        .query("SELECT meta::id(id) AS login, (name ?? '') AS name, profile_picture FROM users WHERE id = $id")
        .bind(("id", RecordId::new("users", login)))
        .await?;

    let row: Option<MeiliUser> = resp.take(0)?;

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

/// Handle a user change event.
pub async fn handle_user_change(
    db: &Db,
    meili: &MeiliIndex,
    operation: &str,
    login: &str,
) {
    let result = match operation {
        "DELETE" => meili.delete_document(login).await,
        "INSERT" | "UPDATE" => sync_single_user(db, meili, login).await,
        other => {
            error!("Unknown user operation: {other}");
            return;
        }
    };

    if let Err(e) = result {
        error!("Failed to handle {operation} for user '{login}': {e}");
    }
}
