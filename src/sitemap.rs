use log::info;
use redis::AsyncCommands;

use crate::db::ScyllaDb;

type RedisConn = redis::aio::ConnectionManager;

pub const SITEMAP_REDIS_KEY: &str = "cache:sitemap";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Generate the sitemap XML from ScyllaDB and store it in Redis under `cache:sitemap`.
pub async fn generate_and_store(
    db: &ScyllaDb,
    redis: &mut RedisConn,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    // Fetch all users
    let user_result = db
        .session
        .query_unpaged("SELECT login FROM users", &[])
        .await?;
    let user_rows = user_result.into_rows_result()?;
    let mut users: Vec<String> = user_rows
        .rows::<(String,)>()?
        .filter_map(|r| r.ok())
        .map(|(login,)| login)
        .collect();
    users.sort();

    for login in &users {
        xml.push_str(&format!(
            "  <url><loc>{}/u/{}</loc><changefreq>weekly</changefreq><priority>0.6</priority></url>\n",
            base_url,
            html_escape(login)
        ));
    }

    // Fetch all media, filter public ones at application level
    let media_result = db
        .session
        .query_unpaged("SELECT id, visibility FROM media", &[])
        .await?;
    let media_rows = media_result.into_rows_result()?;
    let mut media_ids: Vec<String> = media_rows
        .rows::<(String, String)>()?
        .filter_map(|r| r.ok())
        .filter(|(_id, visibility)| visibility == "public")
        .map(|(id, _)| id)
        .collect();
    media_ids.sort();

    for id in &media_ids {
        xml.push_str(&format!(
            "  <url><loc>{}/m/{}</loc><changefreq>monthly</changefreq><priority>0.7</priority></url>\n",
            base_url,
            html_escape(id)
        ));
    }

    // Fetch all lists, filter public ones at application level
    let list_result = db
        .session
        .query_unpaged("SELECT id, visibility FROM lists", &[])
        .await?;
    let list_rows = list_result.into_rows_result()?;
    let mut list_ids: Vec<String> = list_rows
        .rows::<(String, String)>()?
        .filter_map(|r| r.ok())
        .filter(|(_id, visibility)| visibility == "public")
        .map(|(id, _)| id)
        .collect();
    list_ids.sort();

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
