use log::{error, info, warn};
use sqlx::postgres::PgListener;
use sqlx::PgPool;

use crate::meilisearch::MeiliIndex;
use crate::model::ChangeEvent;
use crate::sync;

type RedisConn = redis::aio::ConnectionManager;

/// Listen for PostgreSQL NOTIFY events and dispatch to the appropriate handler.
///
/// `entity` is a human-readable label ("media", "list", "user") used in log messages.
/// `handler` is called with (pool, meili, redis, base_url, operation, id) for each event.
pub async fn listen_for_changes(
    pool: &PgPool,
    meili: &MeiliIndex,
    redis: RedisConn,
    channel: &str,
    entity: &str,
    base_url: &str,
) {
    info!("Setting up PostgreSQL LISTEN on channel '{channel}' for {entity}...");

    let mut redis = redis;
    loop {
        match run_listener(pool, meili, &mut redis, channel, entity, base_url).await {
            Ok(()) => {
                info!("{entity} listener loop ended, restarting...");
            }
            Err(e) => {
                error!("{entity} listener error: {e}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        warn!("Reconnecting {entity} listener...");
    }
}

async fn run_listener(
    pool: &PgPool,
    meili: &MeiliIndex,
    redis: &mut RedisConn,
    channel: &str,
    entity: &str,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(channel).await?;
    info!("Listening for {entity} notifications on '{channel}'");

    loop {
        let notification = listener.recv().await?;
        let payload = notification.payload();

        let event: ChangeEvent = match serde_json::from_str(payload) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to parse {entity} NOTIFY payload '{payload}': {e}");
                continue;
            }
        };

        info!(
            "Received {} event for {entity} '{}'",
            event.operation, event.id
        );

        match entity {
            "media" => {
                sync::handle_change(pool, meili, redis, base_url, &event.operation, &event.id)
                    .await
            }
            "list" => {
                sync::handle_list_change(pool, meili, redis, base_url, &event.operation, &event.id)
                    .await
            }
            "user" => sync::handle_user_change(pool, meili, &event.operation, &event.id).await,
            _ => error!("Unknown entity type: {entity}"),
        }
    }
}
