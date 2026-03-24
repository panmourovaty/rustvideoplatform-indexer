use log::info;
use serde_json::json;

use crate::db::ScyllaDb;
use crate::meilisearch::MeiliIndex;
use crate::model::{MeiliList, MeiliMedia, MeiliUser};

// --- Media ---

/// Count likes and dislikes for a given media ID by querying the media_likes table.
async fn count_reactions(
    db: &ScyllaDb,
    media_id: &str,
) -> Result<(i64, i64), Box<dyn std::error::Error + Send + Sync>> {
    let result = db
        .session
        .execute_unpaged(&db.get_reactions_for_media, (media_id,))
        .await?;
    let rows_result = result.into_rows_result()?;
    let mut likes: i64 = 0;
    let mut dislikes: i64 = 0;
    // Columns: user_login, reaction
    for row in rows_result.rows::<(String, String)>()? {
        let (_user_login, reaction) = row?;
        match reaction.as_str() {
            "like" => likes += 1,
            "dislike" => dislikes += 1,
            _ => {}
        }
    }
    Ok((likes, dislikes))
}

/// Build the `_vectors` payload only for user-provided embedders.
/// REST embedders configured in Meilisearch generate vectors remotely, so
/// documents must not send `_vectors.<embedder>.text`.
fn build_media_vectors(
    embedder_name: &str,
    embedder_source: &str,
    name: &str,
    description: &str,
) -> serde_json::Value {
    if embedder_source.eq_ignore_ascii_case("userProvided") {
        json!({
            embedder_name: {
                "text": format!("{name}\n\n{description}")
            }
        })
    } else {
        json!(null)
    }
}

/// Perform a full sync of all media records from ScyllaDB into Meilisearch.
pub async fn full_sync(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    batch_size: usize,
    embedder_name: &str,
    embedder_source: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting full media sync from ScyllaDB to Meilisearch...");

    let result = db
        .session
        .query_unpaged(
            "SELECT id, name, description, owner, views, type, upload, visibility, restricted_to_group FROM media",
            &[],
        )
        .await?;

    let rows_result = result.into_rows_result()?;
    let total = rows_result.rows_num() as u64;
    info!("Found {total} media records to index");

    if total == 0 {
        info!("No records to sync");
        return Ok(0);
    }

    // Collect all media rows first
    let media_rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        i64,
        String,
        i64,
        String,
        Option<String>,
    )> = rows_result
        .rows::<(
            String,
            String,
            Option<String>,
            String,
            i64,
            String,
            i64,
            String,
            Option<String>,
        )>()?
        .filter_map(|r| r.ok())
        .collect();

    let mut indexed: u64 = 0;
    for chunk in media_rows.chunks(batch_size) {
        let mut batch_docs = Vec::with_capacity(chunk.len());
        for (
            id,
            name,
            description,
            owner,
            views,
            media_type,
            upload,
            visibility,
            restricted_to_group,
        ) in chunk
        {
            let description = description.clone().unwrap_or_default();
            let (likes, dislikes) = count_reactions(db, id).await.unwrap_or((0, 0));
            batch_docs.push(MeiliMedia {
                id: id.clone(),
                name: name.clone(),
                description: description.clone(),
                owner: owner.clone(),
                views: *views,
                likes,
                dislikes,
                r#type: media_type.clone(),
                upload: *upload,
                public: visibility == "public",
                visibility: visibility.clone(),
                restricted_to_group: restricted_to_group.clone(),
                _vectors: build_media_vectors(embedder_name, embedder_source, name, &description),
            });
        }

        let count = batch_docs.len() as u64;
        meili.add_documents(&batch_docs).await?;
        indexed += count;
        info!("Indexed {indexed}/{total} documents");
    }

    info!("Full sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single media record by ID and upsert it into Meilisearch.
pub async fn sync_single(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    media_id: &str,
    embedder_name: &str,
    embedder_source: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = db
        .session
        .execute_unpaged(&db.get_media_by_id, (media_id,))
        .await?;
    let rows_result = result.into_rows_result()?;
    let row = rows_result
        .maybe_first_row::<(
            String,
            String,
            Option<String>,
            String,
            i64,
            String,
            i64,
            String,
            Option<String>,
        )>()?;

    match row {
        Some((
            id,
            name,
            description,
            owner,
            views,
            media_type,
            upload,
            visibility,
            restricted_to_group,
        )) => {
            let description = description.unwrap_or_default();
            let (likes, dislikes) = count_reactions(db, &id).await.unwrap_or((0, 0));
            let doc = MeiliMedia {
                id,
                name: name.clone(),
                description: description.clone(),
                owner,
                views,
                likes,
                dislikes,
                r#type: media_type,
                upload,
                public: visibility == "public",
                visibility,
                restricted_to_group,
                _vectors: build_media_vectors(
                    embedder_name,
                    embedder_source,
                    &name,
                    &description,
                ),
            };
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

// --- Lists ---

/// Count the number of items in a list.
async fn count_items(
    db: &ScyllaDb,
    list_id: &str,
) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let result = db
        .session
        .execute_unpaged(&db.count_list_items, (list_id,))
        .await?;
    let rows_result = result.into_rows_result()?;
    Ok(rows_result.rows_num() as i64)
}

/// Perform a full sync of all non-hidden lists from ScyllaDB into Meilisearch.
pub async fn full_sync_lists(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting full list sync from ScyllaDB to Meilisearch...");

    let result = db
        .session
        .query_unpaged(
            "SELECT id, name, owner, visibility, restricted_to_group, created FROM lists",
            &[],
        )
        .await?;

    let rows_result = result.into_rows_result()?;
    // Collect all rows, then filter out hidden lists at application level
    let list_rows: Vec<(String, String, String, String, Option<String>, i64)> = rows_result
        .rows::<(String, String, String, String, Option<String>, i64)>()?
        .filter_map(|r| r.ok())
        .filter(|(_id, _name, _owner, visibility, _rtg, _created)| visibility != "hidden")
        .collect();

    let total = list_rows.len() as u64;
    info!("Found {total} list records to index");

    if total == 0 {
        info!("No list records to sync");
        return Ok(0);
    }

    let mut indexed: u64 = 0;
    for chunk in list_rows.chunks(batch_size) {
        let mut batch_docs = Vec::with_capacity(chunk.len());
        for (id, name, owner, visibility, restricted_to_group, created) in chunk {
            let item_count = count_items(db, id).await.unwrap_or(0);
            batch_docs.push(MeiliList {
                id: id.clone(),
                name: name.clone(),
                owner: owner.clone(),
                visibility: visibility.clone(),
                restricted_to_group: restricted_to_group.clone(),
                item_count,
                created: *created,
            });
        }

        let count = batch_docs.len() as u64;
        meili.add_documents(&batch_docs).await?;
        indexed += count;
        info!("Indexed {indexed}/{total} list documents");
    }

    info!("Full list sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single list record by ID and upsert/delete it in Meilisearch.
pub async fn sync_single_list(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    list_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = db
        .session
        .execute_unpaged(&db.get_list_by_id, (list_id,))
        .await?;
    let rows_result = result.into_rows_result()?;
    let row = rows_result
        .maybe_first_row::<(String, String, String, String, Option<String>, i64)>()?;

    match row {
        Some((id, name, owner, visibility, restricted_to_group, created)) if visibility != "hidden" => {
            let item_count = count_items(db, &id).await.unwrap_or(0);
            let doc = MeiliList {
                id,
                name,
                owner,
                visibility,
                restricted_to_group,
                item_count,
                created,
            };
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

// --- Users ---

/// Perform a full sync of all users from ScyllaDB into Meilisearch.
pub async fn full_sync_users(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    batch_size: usize,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting full user sync from ScyllaDB to Meilisearch...");

    let result = db
        .session
        .query_unpaged(
            "SELECT login, name, profile_picture FROM users",
            &[],
        )
        .await?;

    let rows_result = result.into_rows_result()?;
    let total = rows_result.rows_num() as u64;
    info!("Found {total} user records to index");

    if total == 0 {
        info!("No user records to sync");
        return Ok(0);
    }

    let user_rows: Vec<(String, String, Option<String>)> = rows_result
        .rows::<(String, String, Option<String>)>()?
        .filter_map(|r| r.ok())
        .collect();

    let mut indexed: u64 = 0;
    for chunk in user_rows.chunks(batch_size) {
        let batch_docs: Vec<MeiliUser> = chunk
            .iter()
            .map(|(login, name, profile_picture)| MeiliUser {
                login: login.clone(),
                name: name.clone(),
                profile_picture: profile_picture.clone(),
            })
            .collect();

        let count = batch_docs.len() as u64;
        meili.add_documents(&batch_docs).await?;
        indexed += count;
        info!("Indexed {indexed}/{total} user documents");
    }

    info!("Full user sync complete: {indexed} documents indexed");
    Ok(indexed)
}

/// Fetch a single user record by login and upsert/delete it in Meilisearch.
pub async fn sync_single_user(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    login: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = db
        .session
        .execute_unpaged(&db.get_user_by_login, (login,))
        .await?;
    let rows_result = result.into_rows_result()?;
    let row = rows_result.maybe_first_row::<(String, String, Option<String>)>()?;

    match row {
        Some((login_val, name, profile_picture)) => {
            let doc = MeiliUser {
                login: login_val,
                name,
                profile_picture,
            };
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
