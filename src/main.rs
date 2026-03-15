mod cache;
mod config;
mod listener;
mod meilisearch;
mod model;
mod sprite;
mod sync;

use log::{error, info};
use surrealdb::engine::remote::ws::{Client as WsClient, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::types::{RecordId, SurrealValue};
use surrealdb::Surreal;

use crate::config::Config;
use crate::meilisearch::MeiliIndex;

type Db = Surreal<WsClient>;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::load();

    info!("Connecting to SurrealDB at {}...", config.surrealdb_url);
    let db: Db = Surreal::new::<Ws>(&config.surrealdb_url)
        .await
        .expect("Failed to connect to SurrealDB");
    db.signin(Root {
        username: config.surrealdb_user.clone(),
        password: config.surrealdb_pass.clone(),
    })
    .await
    .expect("Failed to sign in to SurrealDB");
    db.use_ns(&config.surrealdb_ns)
        .use_db(&config.surrealdb_db)
        .await
        .expect("Failed to select namespace/database");
    info!("Connected to SurrealDB");

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

    // Perform initial full sync for all entity types
    match sync::full_sync(&db, &media_meili, config.batch_size).await {
        Ok(count) => info!("Media sync completed: {count} documents"),
        Err(e) => {
            error!("Media sync failed: {e}");
            std::process::exit(1);
        }
    }
    match sync::full_sync_lists(&db, &lists_meili, config.batch_size).await {
        Ok(count) => info!("List sync completed: {count} documents"),
        Err(e) => {
            error!("List sync failed: {e}");
            std::process::exit(1);
        }
    }
    match sync::full_sync_users(&db, &users_meili, config.batch_size).await {
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

    // Spawn listener tasks for each entity type (each gets its own cloned Db connection)
    let media_db = db.clone();
    let media_meili_clone = media_meili.clone();
    let media_handle = tokio::spawn(async move {
        listener::listen_for_changes(media_db, media_meili_clone, "media").await;
    });

    let list_db = db.clone();
    let lists_meili_clone = lists_meili.clone();
    let list_handle = tokio::spawn(async move {
        listener::listen_for_changes(list_db, lists_meili_clone, "list").await;
    });

    let user_db = db.clone();
    let users_meili_clone = users_meili.clone();
    let user_handle = tokio::spawn(async move {
        listener::listen_for_changes(user_db, users_meili_clone, "user").await;
    });

    // Periodic cache refresh task
    let cache_db = db.clone();
    let source_dir = config.source_dir.clone();
    let sprite_items = config.sprite_items;
    let cache_interval = config.cache_interval_secs;
    let cache_handle = tokio::spawn(async move {
        cache::run_periodic_cache(cache_db, redis_conn, cache_interval, source_dir, sprite_items)
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
    if exit_code != 0 {
        error!("Exiting with code {} due to unexpected task failure", exit_code);
    }
    info!("Goodbye!");
    std::process::exit(exit_code);
}
