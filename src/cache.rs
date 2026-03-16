use log::{error, info};
use redis::AsyncCommands;
use sqlx::PgPool;
use std::time::Duration;

use crate::sprite;

type RedisConn = redis::aio::ConnectionManager;

/// Media data used for building both reaction counts and trending cache.
#[derive(sqlx::FromRow)]
struct MediaCacheData {
    id: String,
    name: String,
    owner: String,
    views: i64,
    #[sqlx(rename = "type")]
    r#type: String,
    visibility: String,
    likes: i64,
    dislikes: i64,
}

/// Periodically refresh the Redis cache with trending metrics and reaction counts.
/// Runs forever, sleeping `interval_secs` between each refresh cycle.
pub async fn run_periodic_cache(
    pool: PgPool,
    redis: RedisConn,
    interval_secs: u64,
    source_dir: String,
    sprite_items: usize,
) {
    info!("Starting periodic cache refresh (interval: {interval_secs}s)");
    let mut redis = redis;
    // Track the last trending list to skip sprite regeneration when unchanged
    let mut last_trending_ids: Vec<String> = Vec::new();
    let mut current_sprite: Option<String> = None;

    loop {
        match refresh_cache(
            &pool,
            &mut redis,
            &source_dir,
            sprite_items,
            &mut last_trending_ids,
            &mut current_sprite,
        )
        .await
        {
            Ok(()) => info!("Cache refresh completed"),
            Err(e) => error!("Cache refresh failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}

/// Single cache refresh cycle: fetch all media with reaction counts from DB,
/// then update Redis with reaction counts and trending sorted set.
/// Also regenerates the trending sprite if the top items have changed.
async fn refresh_cache(
    pool: &PgPool,
    redis: &mut RedisConn,
    source_dir: &str,
    sprite_items: usize,
    last_trending_ids: &mut Vec<String>,
    current_sprite: &mut Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Single query: join media with aggregated reaction counts
    let all_media: Vec<MediaCacheData> = sqlx::query_as(
        "SELECT m.id, m.name, m.owner, m.views, m.type, m.visibility, \
         COUNT(*) FILTER (WHERE ml.reaction = 'like') AS likes, \
         COUNT(*) FILTER (WHERE ml.reaction = 'dislike') AS dislikes \
         FROM media m \
         LEFT JOIN media_likes ml ON m.id = ml.media_id \
         GROUP BY m.id, m.name, m.owner, m.views, m.type, m.visibility \
         ORDER BY likes DESC",
    )
    .fetch_all(pool)
    .await?;

    if all_media.is_empty() {
        let _: Result<(), _> = redis.del("cache:trending").await;
        info!("Cache refresh: no media found");
        return Ok(());
    }

    let temp_trending_key = "cache:trending:tmp";

    // Delete temp key if leftover from a previous failed run
    let _: Result<(), _> = redis.del(temp_trending_key).await;

    // Process in batches to avoid excessively large pipelines
    let batch_size = 500;
    let mut trending_ids: Vec<String> = Vec::new();
    let mut reaction_count = 0u64;

    for chunk in all_media.chunks(batch_size) {
        let mut pipe = redis::pipe();

        for item in chunk {
            // Cache reaction counts for every media item
            pipe.set(format!("cache:media:{}:likes", item.id), item.likes)
                .ignore();
            pipe.set(format!("cache:media:{}:dislikes", item.id), item.dislikes)
                .ignore();
            // Set TTL so orphaned keys expire if media is deleted
            pipe.expire(format!("cache:media:{}:likes", item.id), 3600)
                .ignore();
            pipe.expire(format!("cache:media:{}:dislikes", item.id), 3600)
                .ignore();
            reaction_count += 1;

            // Only public media goes into the trending sorted set
            if item.visibility == "public" {
                pipe.zadd(temp_trending_key, &item.id, item.likes)
                    .ignore();

                let info_key = format!("cache:trending:info:{}", item.id);
                pipe.hset(&info_key, "name", &item.name).ignore();
                pipe.hset(&info_key, "owner", &item.owner).ignore();
                pipe.hset(&info_key, "views", item.views).ignore();
                pipe.hset(&info_key, "type", &item.r#type).ignore();
                pipe.expire(&info_key, 300).ignore();
                trending_ids.push(item.id.clone());
            }
        }

        let _: () = pipe.query_async(redis).await?;
    }

    // Atomic swap: replace the live trending set with the freshly built one
    if !trending_ids.is_empty() {
        let _: () = redis.rename(temp_trending_key, "cache:trending").await?;
    } else {
        let _: Result<(), _> = redis.del(temp_trending_key).await;
        let _: Result<(), _> = redis.del("cache:trending").await;
    }

    info!(
        "Cache refresh: {} reaction counts, {} trending items",
        reaction_count,
        trending_ids.len()
    );

    // Determine the top N trending IDs for the sprite
    let top_ids: Vec<String> = trending_ids
        .iter()
        .take(sprite_items)
        .cloned()
        .collect();

    // Regenerate sprite only if the top list has changed
    if top_ids != *last_trending_ids {
        info!(
            "Trending list changed ({} -> {} items), regenerating sprite",
            last_trending_ids.len(),
            top_ids.len()
        );

        let old_sprite = current_sprite.take();
        let source_dir_owned = source_dir.to_string();
        let top_ids_owned = top_ids.clone();

        // Run sprite generation on a blocking thread to avoid blocking the async runtime
        let new_sprite = tokio::task::spawn_blocking(move || {
            sprite::generate_trending_sprite(&source_dir_owned, &top_ids_owned)
        })
        .await
        .unwrap_or(None);

        if let Some((ref sprite_name, ref included_ids)) = new_sprite {
            // Store the new sprite filename in Redis for the main app to use
            let _: Result<(), _> = redis
                .set::<_, _, ()>("cache:trending:sprite", sprite_name)
                .await;

            // Store per-media sprite positions based on actual inclusion order
            let sprite_w: i32 = 352;
            let sprite_h: i32 = 198;
            let sprite_cols: i32 = 5;
            for (i, id) in included_ids.iter().enumerate() {
                let i = i as i32;
                let sx = -((i % sprite_cols) * sprite_w);
                let sy = -((i / sprite_cols) * sprite_h);
                let key = format!("cache:trending:info:{}", id);
                let _: Result<(), _> = redis.hset(&key, "sprite_x", sx).await;
                let _: Result<(), _> = redis.hset(&key, "sprite_y", sy).await;
            }

            info!("Updated trending sprite in Redis: {}", sprite_name);
        }

        // Delete the old sprite file after the new one is ready
        if let Some(old_name) = old_sprite {
            let source_dir_owned = source_dir.to_string();
            tokio::task::spawn_blocking(move || {
                sprite::delete_sprite(&source_dir_owned, &old_name);
            })
            .await
            .ok();
        }

        *current_sprite = new_sprite.map(|(name, _)| name);
        *last_trending_ids = top_ids;
    } else {
        info!("Trending list unchanged, skipping sprite regeneration");
    }

    Ok(())
}
