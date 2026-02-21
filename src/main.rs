mod config;
mod listener;
mod meilisearch;
mod model;
mod sync;

use log::{error, info};
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::meilisearch::MeiliIndex;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::load();

    info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");
    info!("Connected to PostgreSQL");

    info!("Connecting to Meilisearch at {}...", config.meilisearch_url);
    let meili = MeiliIndex::new(&config.meilisearch_url, config.meilisearch_key.as_deref());

    // Configure the Meilisearch index (searchable, filterable, sortable attributes)
    meili
        .setup_index()
        .await
        .expect("Failed to configure Meilisearch index");

    // Ensure the PostgreSQL trigger and function exist for LISTEN/NOTIFY
    setup_notify_trigger(&pool, &config.notify_channel).await;

    // Perform initial full sync
    match sync::full_sync(&pool, &meili, config.batch_size).await {
        Ok(count) => info!("Initial sync completed: {count} documents"),
        Err(e) => {
            error!("Initial sync failed: {e}");
            std::process::exit(1);
        }
    }

    info!("Starting change listener...");
    info!("Press Ctrl+C to stop");

    // Listen for changes in a separate task
    let listener_pool = pool.clone();
    let listener_handle = tokio::spawn(async move {
        listener::listen_for_changes(&listener_pool, &meili, &config.notify_channel).await;
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    info!("Shutting down...");
    listener_handle.abort();
    pool.close().await;
    info!("Goodbye!");
}

/// Create the PostgreSQL trigger function and trigger for LISTEN/NOTIFY
/// if they don't already exist.
async fn setup_notify_trigger(pool: &sqlx::PgPool, channel: &str) {
    info!("Ensuring PostgreSQL notify trigger exists...");

    let function_sql = format!(
        r#"
        CREATE OR REPLACE FUNCTION notify_media_changes() RETURNS trigger AS $$
        BEGIN
            IF TG_OP = 'DELETE' THEN
                PERFORM pg_notify('{channel}', json_build_object('operation', TG_OP, 'id', OLD.id)::text);
                RETURN OLD;
            ELSE
                PERFORM pg_notify('{channel}', json_build_object('operation', TG_OP, 'id', NEW.id)::text);
                RETURN NEW;
            END IF;
        END;
        $$ LANGUAGE plpgsql;
        "#
    );

    if let Err(e) = sqlx::query(&function_sql).execute(pool).await {
        error!("Failed to create notify function: {e}");
        error!("Change detection will not work. Create the trigger manually.");
        return;
    }

    let trigger_sql = r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_trigger WHERE tgname = 'media_notify_trigger'
            ) THEN
                CREATE TRIGGER media_notify_trigger
                    AFTER INSERT OR UPDATE OR DELETE ON media
                    FOR EACH ROW EXECUTE FUNCTION notify_media_changes();
            END IF;
        END;
        $$;
    "#;

    if let Err(e) = sqlx::query(trigger_sql).execute(pool).await {
        error!("Failed to create notify trigger: {e}");
        error!("Change detection will not work. Create the trigger manually.");
        return;
    }

    info!("PostgreSQL notify trigger is ready");
}
