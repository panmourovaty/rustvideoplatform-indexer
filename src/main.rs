mod cache;
mod config;
mod listener;
mod meilisearch;
mod model;
mod sprite;
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
        .max_connections(10)
        .connect(&config.dbconnection)
        .await
        .expect("Failed to connect to PostgreSQL");
    info!("Connected to PostgreSQL");

    info!("Connecting to Meilisearch at {}...", config.meilisearch_url);
    let media_meili = MeiliIndex::new(
        &config.meilisearch_url,
        config.meilisearch_key.as_deref(),
        "media",
        "id",
    );
    let lists_meili = MeiliIndex::new(
        &config.meilisearch_url,
        config.meilisearch_key.as_deref(),
        "lists",
        "id",
    );
    let users_meili = MeiliIndex::new(
        &config.meilisearch_url,
        config.meilisearch_key.as_deref(),
        "users",
        "login",
    );

    // Configure all Meilisearch indexes
    media_meili
        .setup_media_index()
        .await
        .expect("Failed to configure media index");
    lists_meili
        .setup_lists_index()
        .await
        .expect("Failed to configure lists index");
    users_meili
        .setup_users_index()
        .await
        .expect("Failed to configure users index");

    // Ensure PostgreSQL triggers exist for all three tables
    setup_media_notify_trigger(&pool, &config.notify_channel).await;
    setup_list_notify_trigger(&pool, &config.list_notify_channel).await;
    setup_list_items_notify_trigger(&pool, &config.list_notify_channel).await;
    setup_user_notify_trigger(&pool, &config.user_notify_channel).await;

    // Perform initial full sync for all entity types
    match sync::full_sync(&pool, &media_meili, config.batch_size).await {
        Ok(count) => info!("Media sync completed: {count} documents"),
        Err(e) => {
            error!("Media sync failed: {e}");
            std::process::exit(1);
        }
    }
    match sync::full_sync_lists(&pool, &lists_meili, config.batch_size).await {
        Ok(count) => info!("List sync completed: {count} documents"),
        Err(e) => {
            error!("List sync failed: {e}");
            std::process::exit(1);
        }
    }
    match sync::full_sync_users(&pool, &users_meili, config.batch_size).await {
        Ok(count) => info!("User sync completed: {count} documents"),
        Err(e) => {
            error!("User sync failed: {e}");
            std::process::exit(1);
        }
    }

    // Connect to Redis/Dragonfly for caching
    info!("Connecting to Redis at {}...", config.redis_url);
    let redis_client =
        redis::Client::open(config.redis_url.as_str()).expect("Invalid Redis URL");
    let redis_conn = redis_client
        .get_connection_manager()
        .await
        .expect("Failed to connect to Redis");
    info!("Connected to Redis");

    info!("Starting change listeners and cache refresh...");
    info!("Send SIGTERM or SIGINT (Ctrl+C) to stop");

    // Spawn listener tasks for each entity type
    let media_pool = pool.clone();
    let media_channel = config.notify_channel.clone();
    let media_handle = tokio::spawn(async move {
        listener::listen_for_changes(&media_pool, &media_meili, &media_channel, "media").await;
    });

    let list_pool = pool.clone();
    let list_channel = config.list_notify_channel.clone();
    let list_handle = tokio::spawn(async move {
        listener::listen_for_changes(&list_pool, &lists_meili, &list_channel, "list").await;
    });

    let user_pool = pool.clone();
    let user_channel = config.user_notify_channel.clone();
    let user_handle = tokio::spawn(async move {
        listener::listen_for_changes(&user_pool, &users_meili, &user_channel, "user").await;
    });

    // Periodic cache refresh task
    let cache_pool = pool.clone();
    let source_dir = config.source_dir.clone();
    let sprite_items = config.sprite_items;
    let cache_interval = config.cache_interval_secs;
    let cache_handle = tokio::spawn(async move {
        cache::run_periodic_cache(cache_pool, redis_conn, cache_interval, source_dir, sprite_items)
            .await;
    });

    // Wait for shutdown signal (SIGINT or SIGTERM) or unexpected task exit
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");

    let exit_code;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT (Ctrl+C), shutting down...");
            exit_code = 0;
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down...");
            exit_code = 0;
        }
        result = media_handle => {
            error!("Media listener task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
        result = list_handle => {
            error!("List listener task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
        result = user_handle => {
            error!("User listener task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
        result = cache_handle => {
            error!("Cache task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
    }

    info!("Shutting down...");
    pool.close().await;
    if exit_code != 0 {
        error!("Exiting with code {} due to unexpected task failure", exit_code);
    }
    info!("Goodbye!");
    std::process::exit(exit_code);
}

/// Create the PostgreSQL trigger for media changes.
async fn setup_media_notify_trigger(pool: &sqlx::PgPool, channel: &str) {
    info!("Ensuring PostgreSQL media notify trigger exists...");

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
        error!("Failed to create media notify function: {e}");
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
        error!("Failed to create media notify trigger: {e}");
        return;
    }

    info!("PostgreSQL media notify trigger is ready");
}

/// Create the PostgreSQL trigger for list changes.
async fn setup_list_notify_trigger(pool: &sqlx::PgPool, channel: &str) {
    info!("Ensuring PostgreSQL list notify trigger exists...");

    let function_sql = format!(
        r#"
        CREATE OR REPLACE FUNCTION notify_list_changes() RETURNS trigger AS $$
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
        error!("Failed to create list notify function: {e}");
        return;
    }

    let trigger_sql = r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_trigger WHERE tgname = 'list_notify_trigger'
            ) THEN
                CREATE TRIGGER list_notify_trigger
                    AFTER INSERT OR UPDATE OR DELETE ON lists
                    FOR EACH ROW EXECUTE FUNCTION notify_list_changes();
            END IF;
        END;
        $$;
    "#;

    if let Err(e) = sqlx::query(trigger_sql).execute(pool).await {
        error!("Failed to create list notify trigger: {e}");
        return;
    }

    info!("PostgreSQL list notify trigger is ready");
}

/// Create the PostgreSQL trigger on list_items so item_count stays fresh.
/// Fires an UPDATE event on the parent list whenever items are added/removed.
async fn setup_list_items_notify_trigger(pool: &sqlx::PgPool, channel: &str) {
    info!("Ensuring PostgreSQL list_items notify trigger exists...");

    let function_sql = format!(
        r#"
        CREATE OR REPLACE FUNCTION notify_list_item_changes() RETURNS trigger AS $$
        BEGIN
            IF TG_OP = 'DELETE' THEN
                PERFORM pg_notify('{channel}', json_build_object('operation', 'UPDATE', 'id', OLD.list_id)::text);
                RETURN OLD;
            ELSE
                PERFORM pg_notify('{channel}', json_build_object('operation', 'UPDATE', 'id', NEW.list_id)::text);
                RETURN NEW;
            END IF;
        END;
        $$ LANGUAGE plpgsql;
        "#
    );

    if let Err(e) = sqlx::query(&function_sql).execute(pool).await {
        error!("Failed to create list_items notify function: {e}");
        return;
    }

    let trigger_sql = r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_trigger WHERE tgname = 'list_item_notify_trigger'
            ) THEN
                CREATE TRIGGER list_item_notify_trigger
                    AFTER INSERT OR DELETE ON list_items
                    FOR EACH ROW EXECUTE FUNCTION notify_list_item_changes();
            END IF;
        END;
        $$;
    "#;

    if let Err(e) = sqlx::query(trigger_sql).execute(pool).await {
        error!("Failed to create list_items notify trigger: {e}");
        return;
    }

    info!("PostgreSQL list_items notify trigger is ready");
}

/// Create the PostgreSQL trigger for user changes.
async fn setup_user_notify_trigger(pool: &sqlx::PgPool, channel: &str) {
    info!("Ensuring PostgreSQL user notify trigger exists...");

    let function_sql = format!(
        r#"
        CREATE OR REPLACE FUNCTION notify_user_changes() RETURNS trigger AS $$
        BEGIN
            IF TG_OP = 'DELETE' THEN
                PERFORM pg_notify('{channel}', json_build_object('operation', TG_OP, 'id', OLD.login)::text);
                RETURN OLD;
            ELSE
                PERFORM pg_notify('{channel}', json_build_object('operation', TG_OP, 'id', NEW.login)::text);
                RETURN NEW;
            END IF;
        END;
        $$ LANGUAGE plpgsql;
        "#
    );

    if let Err(e) = sqlx::query(&function_sql).execute(pool).await {
        error!("Failed to create user notify function: {e}");
        return;
    }

    let trigger_sql = r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_trigger WHERE tgname = 'user_notify_trigger'
            ) THEN
                CREATE TRIGGER user_notify_trigger
                    AFTER INSERT OR UPDATE OR DELETE ON users
                    FOR EACH ROW EXECUTE FUNCTION notify_user_changes();
            END IF;
        END;
        $$;
    "#;

    if let Err(e) = sqlx::query(trigger_sql).execute(pool).await {
        error!("Failed to create user notify trigger: {e}");
        return;
    }

    info!("PostgreSQL user notify trigger is ready");
}
