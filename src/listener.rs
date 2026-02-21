use log::{error, info, warn};
use sqlx::postgres::PgListener;
use sqlx::PgPool;

use crate::meilisearch::MeiliIndex;
use crate::model::MediaChangeEvent;
use crate::sync;

/// Listen for PostgreSQL NOTIFY events on the given channel and apply changes
/// to Meilisearch in real-time.
///
/// The expected NOTIFY payload is JSON: `{"operation": "INSERT|UPDATE|DELETE", "id": "<media_id>"}`
///
/// To enable this, create a trigger in PostgreSQL:
///
/// ```sql
/// CREATE OR REPLACE FUNCTION notify_media_changes() RETURNS trigger AS $$
/// BEGIN
///   IF TG_OP = 'DELETE' THEN
///     PERFORM pg_notify('media_changes', json_build_object('operation', TG_OP, 'id', OLD.id)::text);
///     RETURN OLD;
///   ELSE
///     PERFORM pg_notify('media_changes', json_build_object('operation', TG_OP, 'id', NEW.id)::text);
///     RETURN NEW;
///   END IF;
/// END;
/// $$ LANGUAGE plpgsql;
///
/// CREATE TRIGGER media_notify_trigger
///   AFTER INSERT OR UPDATE OR DELETE ON media
///   FOR EACH ROW EXECUTE FUNCTION notify_media_changes();
/// ```
pub async fn listen_for_changes(pool: &PgPool, meili: &MeiliIndex, channel: &str) {
    info!("Setting up PostgreSQL LISTEN on channel '{channel}'...");

    loop {
        match run_listener(pool, meili, channel).await {
            Ok(()) => {
                info!("Listener loop ended, restarting...");
            }
            Err(e) => {
                error!("Listener error: {e}");
            }
        }
        // Brief pause before reconnecting
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        warn!("Reconnecting listener...");
    }
}

async fn run_listener(
    pool: &PgPool,
    meili: &MeiliIndex,
    channel: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(channel).await?;
    info!("Listening for notifications on '{channel}'");

    loop {
        let notification = listener.recv().await?;
        let payload = notification.payload();

        let event: MediaChangeEvent = match serde_json::from_str(payload) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to parse NOTIFY payload '{payload}': {e}");
                continue;
            }
        };

        info!(
            "Received {} event for media '{}'",
            event.operation, event.id
        );

        sync::handle_change(pool, meili, &event.operation, &event.id).await;
    }
}
