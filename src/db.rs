use scylla::statement::prepared::PreparedStatement;
use scylla::client::session::Session;
use std::sync::Arc;

pub struct ScyllaDb {
    pub session: Arc<Session>,
    // Single-row lookups (prepared statements)
    pub get_media_by_id: PreparedStatement,
    pub get_media_description: PreparedStatement,
    pub get_reactions_for_media: PreparedStatement,
    pub get_subtitle_for_media: PreparedStatement,
    pub get_list_by_id: PreparedStatement,
    pub count_list_items: PreparedStatement,
    pub get_user_by_login: PreparedStatement,
}

impl ScyllaDb {
    pub async fn connect(
        nodes: &[String],
        keyspace: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let session = scylla::client::session_builder::SessionBuilder::new()
            .known_nodes(nodes)
            .use_keyspace(keyspace, false)
            .build()
            .await?;
        let session = Arc::new(session);

        Ok(ScyllaDb {
            get_media_by_id: session
                .prepare(
                    "SELECT id, name, description, owner, views, type, upload, visibility, restricted_to_group \
                     FROM media WHERE id = ?",
                )
                .await?,
            get_media_description: session
                .prepare("SELECT description FROM media WHERE id = ?")
                .await?,
            get_reactions_for_media: session
                .prepare("SELECT user_login, reaction FROM media_likes WHERE media_id = ?")
                .await?,
            get_subtitle_for_media: session
                .prepare("SELECT language, content FROM media_subtitles WHERE media_id = ?")
                .await?,
            get_list_by_id: session
                .prepare(
                    "SELECT id, name, owner, visibility, restricted_to_group, created \
                     FROM lists WHERE id = ?",
                )
                .await?,
            count_list_items: session
                .prepare("SELECT media_id FROM list_items WHERE list_id = ?")
                .await?,
            get_user_by_login: session
                .prepare("SELECT login, name, profile_picture FROM users WHERE login = ?")
                .await?,
            session,
        })
    }
}
