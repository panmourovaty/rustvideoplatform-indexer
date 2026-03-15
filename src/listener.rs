use futures::StreamExt;
use log::{error, info, warn};
use serde::Deserialize;
use surrealdb::engine::remote::ws::Client as WsClient;
use surrealdb::{Action, Notification, Surreal};

use crate::meilisearch::MeiliIndex;
use crate::sync;

type Db = Surreal<WsClient>;

/// Generic record with just an id field, used to extract the ID from delete notifications.
#[derive(Debug, Deserialize)]
struct IdRecord {
    id: surrealdb::RecordId,
}

/// Listen for SurrealDB live query events and dispatch to the appropriate handler.
/// `entity` is a human-readable label ("media", "list", "user") used in log messages.
pub async fn listen_for_changes(
    db: Db,
    meili: MeiliIndex,
    entity: &'static str,
) {
    info!("Setting up SurrealDB live query for {entity}...");

    loop {
        match run_listener(&db, &meili, entity).await {
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
    db: &Db,
    meili: &MeiliIndex,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let table = match entity {
        "media" => "media",
        "list" => "lists",
        "user" => "users",
        _ => return Err(format!("Unknown entity: {entity}").into()),
    };

    info!("Starting live select on table '{table}' for {entity}");

    let mut stream = db.select(table).live().await?;

    info!("Listening for {entity} changes via live query on '{table}'");

    while let Some(event) = stream.next().await {
        let notification: Notification<serde_json::Value> = match event {
            Ok(n) => n,
            Err(e) => {
                error!("{entity} live query stream error: {e}");
                return Err(e.into());
            }
        };

        // Extract the record ID key string
        let record_id_str: Option<String> = notification
            .data
            .get("id")
            .and_then(|v| {
                // SurrealDB SDK may serialize RecordId as {"tb":"table","id":"key"} or as string
                if let Some(obj) = v.as_object() {
                    obj.get("id").and_then(|k| k.as_str()).map(|s| s.to_string())
                } else {
                    v.as_str().map(|s| {
                        // Strip table prefix if present (e.g. "media:abc" -> "abc")
                        if let Some(pos) = s.find(':') {
                            s[pos + 1..].to_string()
                        } else {
                            s.to_string()
                        }
                    })
                }
            });

        let record_id = match record_id_str {
            Some(id) => id,
            None => {
                error!("{entity} live event missing id: {:?}", notification.data);
                continue;
            }
        };

        let operation = match notification.action {
            Action::Create | Action::Update => "UPDATE",
            Action::Delete => "DELETE",
            _ => {
                warn!("{entity} unknown action for '{record_id}', skipping");
                continue;
            }
        };

        info!("Received {operation} event for {entity} '{record_id}'");

        match entity {
            "media" => sync::handle_change(db, meili, operation, &record_id).await,
            "list" => sync::handle_list_change(db, meili, operation, &record_id).await,
            "user" => sync::handle_user_change(db, meili, operation, &record_id).await,
            _ => error!("Unknown entity type: {entity}"),
        }
    }

    Ok(())
}
