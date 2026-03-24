use log::info;

use crate::db::ScyllaDb;
use crate::meilisearch::MeiliIndex;
use crate::sync;

/// Periodically re-sync all documents from ScyllaDB to Meilisearch.
/// Since ScyllaDB doesn't support LISTEN/NOTIFY, we poll on a fixed interval.
pub async fn poll_for_changes(
    db: &ScyllaDb,
    meili: &MeiliIndex,
    entity: &str,
    interval_secs: u64,
    media_embedder_name: Option<String>,
    media_embedder_source: Option<String>,
) {
    info!("Starting polling-based sync for {entity} (interval: {interval_secs}s)");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        info!("Running periodic {entity} sync...");
        match entity {
            "media" => {
                let embedder_name = media_embedder_name.as_deref().unwrap_or("default");
                let embedder_source = media_embedder_source.as_deref().unwrap_or("userProvided");
                if let Err(e) = sync::full_sync(db, meili, 1000, embedder_name, embedder_source).await {
                    log::error!("Media periodic sync failed: {e}");
                }
            }
            "list" => {
                if let Err(e) = sync::full_sync_lists(db, meili, 1000).await {
                    log::error!("List periodic sync failed: {e}");
                }
            }
            "user" => {
                if let Err(e) = sync::full_sync_users(db, meili, 1000).await {
                    log::error!("User periodic sync failed: {e}");
                }
            }
            _ => {}
        }
    }
}
