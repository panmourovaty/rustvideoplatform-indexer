use log::{error, info};
use redis::AsyncCommands;
use sqlx::PgPool;
use std::time::Duration;

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
pub async fn run_periodic_cache(pool: PgPool, redis: RedisConn, interval_secs: u64) {
    info!("Starting periodic cache refresh (interval: {interval_secs}s)");
    let mut redis = redis;

    loop {
        match refresh_cache(&pool, &mut redis).await {
            Ok(()) => info!("Cache refresh completed"),
            Err(e) => error!("Cache refresh failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}

/// Single cache refresh cycle: fetch all media with reaction counts from DB,
/// then update Redis with reaction counts and trending sorted set.
async fn refresh_cache(
    pool: &PgPool,
    redis: &mut RedisConn,
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
    let mut trending_count = 0u64;
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
                trending_count += 1;
            }
        }

        let _: () = pipe.query_async(redis).await?;
    }

    // Atomic swap: replace the live trending set with the freshly built one
    if trending_count > 0 {
        let _: () = redis.rename(temp_trending_key, "cache:trending").await?;
    } else {
        let _: Result<(), _> = redis.del(temp_trending_key).await;
        let _: Result<(), _> = redis.del("cache:trending").await;
    }

    info!(
        "Cache refresh: {} reaction counts, {} trending items",
        reaction_count, trending_count
    );
    Ok(())
}
