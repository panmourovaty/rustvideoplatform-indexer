use log::info;
use redis::AsyncCommands;
use sqlx::PgPool;

type RedisConn = redis::aio::ConnectionManager;

pub const SITEMAP_REDIS_KEY: &str = "cache:sitemap";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Generate the sitemap XML from PostgreSQL and store it in Redis under `cache:sitemap`.
pub async fn generate_and_store(
    pool: &PgPool,
    redis: &mut RedisConn,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");

    xml.push_str(&format!(
        "  <url><loc>{}/</loc><changefreq>daily</changefreq><priority>1.0</priority></url>\n",
        base_url
    ));
    xml.push_str(&format!(
        "  <url><loc>{}/trending</loc><changefreq>daily</changefreq><priority>0.8</priority></url>\n",
        base_url
    ));
    xml.push_str(&format!(
        "  <url><loc>{}/search</loc><changefreq>weekly</changefreq><priority>0.5</priority></url>\n",
        base_url
    ));

    let users: Vec<String> = sqlx::query_scalar("SELECT login FROM users ORDER BY login")
        .fetch_all(pool)
        .await?;

    for login in &users {
        xml.push_str(&format!(
            "  <url><loc>{}/u/{}</loc><changefreq>weekly</changefreq><priority>0.6</priority></url>\n",
            base_url,
            html_escape(login)
        ));
    }

    let media_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM media WHERE visibility = 'public' ORDER BY upload DESC",
    )
    .fetch_all(pool)
    .await?;

    for id in &media_ids {
        xml.push_str(&format!(
            "  <url><loc>{}/m/{}</loc><changefreq>monthly</changefreq><priority>0.7</priority></url>\n",
            base_url,
            html_escape(id)
        ));
    }

    let list_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM lists WHERE visibility = 'public' ORDER BY created DESC",
    )
    .fetch_all(pool)
    .await?;

    for id in &list_ids {
        xml.push_str(&format!(
            "  <url><loc>{}/l/{}</loc><changefreq>weekly</changefreq><priority>0.6</priority></url>\n",
            base_url,
            html_escape(id)
        ));
    }

    xml.push_str("</urlset>\n");

    let _: () = redis.set(SITEMAP_REDIS_KEY, &xml).await?;
    info!(
        "Sitemap stored in Redis ({} users, {} media, {} lists)",
        users.len(),
        media_ids.len(),
        list_ids.len()
    );

    Ok(())
}
