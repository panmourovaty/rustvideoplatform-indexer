mod cache;
mod config;
mod db;
mod listener;
mod meilisearch;
mod model;
mod sitemap;
mod sprite;
mod sync;

use log::{error, info};
use std::sync::Arc;

use crate::config::Config;
use crate::db::ScyllaDb;
use crate::meilisearch::MeiliIndex;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::load();

    info!("Connecting to ScyllaDB...");
    let db = ScyllaDb::connect(&config.scylla_nodes, &config.scylla_keyspace)
        .await
        .expect("Failed to connect to ScyllaDB");
    let db = Arc::new(db);
    info!("Connected to ScyllaDB");

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
        .setup_media_index(&config.meilisearch_embedder)
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
    match sync::full_sync(
        &db,
        &media_meili,
        config.batch_size,
        &config.meilisearch_embedder.name,
    )
    .await
    {
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

    // Generate the initial sitemap now that all data is synced
    info!("Generating initial sitemap...");
    let mut redis_init = redis_conn.clone();
    if let Err(e) = sitemap::generate_and_store(&db, &mut redis_init, &config.site_url).await {
        error!("Failed to generate initial sitemap: {e}");
    }

    info!("Starting polling-based sync and cache refresh...");
    info!("Send SIGTERM or SIGINT (Ctrl+C) to stop");

    let poll_interval = config.poll_interval_secs;

    // Spawn polling tasks for each entity type
    let media_db = Arc::clone(&db);
    let media_embedder = config.meilisearch_embedder.name.clone();
    let media_handle = tokio::spawn(async move {
        listener::poll_for_changes(
            &media_db,
            &media_meili,
            "media",
            poll_interval,
            Some(media_embedder),
        )
        .await;
    });

    let list_db = Arc::clone(&db);
    let list_handle = tokio::spawn(async move {
        listener::poll_for_changes(&list_db, &lists_meili, "list", poll_interval, None).await;
    });

    let user_db = Arc::clone(&db);
    let user_handle = tokio::spawn(async move {
        listener::poll_for_changes(&user_db, &users_meili, "user", poll_interval, None).await;
    });

    // Periodic cache refresh task
    let cache_db = Arc::clone(&db);
    let source_dir = config.source_dir.clone();
    let sprite_items = config.sprite_items;
    let cache_interval = config.cache_interval_secs;
    let cache_site_url = config.site_url.clone();
    let cache_handle = tokio::spawn(async move {
        cache::run_periodic_cache(
            &cache_db,
            redis_conn,
            cache_interval,
            source_dir,
            sprite_items,
            cache_site_url,
        )
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
            error!("Media poller task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
        result = list_handle => {
            error!("List poller task exited unexpectedly: {:?}", result);
            exit_code = 1;
        }
        result = user_handle => {
            error!("User poller task exited unexpectedly: {:?}", result);
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
