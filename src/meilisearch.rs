use log::{error, info};
use meilisearch_sdk::client::Client;
use meilisearch_sdk::errors::ErrorCode;
use meilisearch_sdk::tasks::Task;
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::config::MeilisearchEmbedderConfig;

pub struct MeiliIndex {
    client: Client,
    index_name: String,
    primary_key: String,
    base_url: String,
    api_key: Option<String>,
    http_client: reqwest::Client,
}

impl MeiliIndex {
    pub fn new(url: &str, key: Option<&str>, index_name: &str, primary_key: &str) -> Self {
        let client = Client::new(url, key).expect("Failed to create Meilisearch client");
        MeiliIndex {
            client,
            index_name: index_name.to_owned(),
            primary_key: primary_key.to_owned(),
            base_url: url.trim_end_matches('/').to_owned(),
            api_key: key.map(str::to_owned),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn index_name(&self) -> &str {
        &self.index_name
    }

    /// Ensure the index exists, creating it if necessary.
    async fn ensure_index_exists(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let task = self
            .client
            .create_index(&self.index_name, Some(&self.primary_key))
            .await?;
        let task = task
            .wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300)))
            .await?;
        match task {
            Task::Failed { content } if content.error.error_code == ErrorCode::IndexAlreadyExists => {
                info!("Meilisearch index '{}' already exists", self.index_name);
                Ok(())
            }
            Task::Failed { content } => {
                Err(format!(
                    "Failed to create index '{}': {}",
                    self.index_name, content.error
                )
                .into())
            }
            _ => Ok(()),
        }
    }

    /// Log a reminder about the Meilisearch REST-embedder timeout.
    ///
    /// The timeout cannot be set via any Meilisearch API call — it requires the
    /// environment variable `MEILI_EXPERIMENTAL_REST_EMBEDDER_TIMEOUT_SECONDS`
    /// to be set on the Meilisearch server at startup (≥ v1.26.0).
    /// The default is 30 seconds, which is easily exceeded when embedding large
    /// subtitle files.  This function logs an info message reminding operators
    /// to verify the server is configured correctly.
    pub fn log_embedding_timeout_reminder(timeout_secs: u64) {
        info!(
            "Embedding timeout reminder: ensure the Meilisearch server has \
             MEILI_EXPERIMENTAL_REST_EMBEDDER_TIMEOUT_SECONDS={timeout_secs} set \
             (Meilisearch ≥ v1.26.0, default is 30s which may be too short for subtitle indexing)."
        );
    }

    /// Configure the "media" index with its specific settings.
    pub async fn setup_media_index(
        &self,
        embedder_config: &MeilisearchEmbedderConfig,
        embedding_timeout_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        self.ensure_index_exists().await?;

        let index = self.client.index(&self.index_name);

        let task = index
            .set_searchable_attributes(["name", "description", "subtitle", "owner"])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_filterable_attributes([
                "public",
                "type",
                "upload",
                "views",
                "likes",
                "visibility",
                "restricted_to_group",
                "owner",
            ])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_sortable_attributes(["upload", "views", "likes"])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_ranking_rules([
                "words",
                "typo",
                "proximity",
                "attribute",
                "sort",
                "exactness",
            ])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        self.configure_embedders_via_http(embedder_config).await?;
        Self::log_embedding_timeout_reminder(embedding_timeout_secs);

        info!(
            "Meilisearch index '{}' configured successfully",
            self.index_name
        );
        Ok(())
    }

    /// Configure the "lists" index.
    pub async fn setup_lists_index(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        self.ensure_index_exists().await?;

        let index = self.client.index(&self.index_name);

        let task = index.set_searchable_attributes(["name", "owner"]).await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_filterable_attributes(["visibility", "restricted_to_group", "owner"])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_sortable_attributes(["created", "item_count"])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_ranking_rules([
                "words",
                "typo",
                "proximity",
                "attribute",
                "sort",
                "exactness",
            ])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        info!(
            "Meilisearch index '{}' configured successfully",
            self.index_name
        );
        Ok(())
    }

    /// Configure the "users" index.
    pub async fn setup_users_index(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        self.ensure_index_exists().await?;

        let index = self.client.index(&self.index_name);

        let task = index
            .set_searchable_attributes(["name", "login"])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        let task = index
            .set_ranking_rules([
                "words",
                "typo",
                "proximity",
                "attribute",
                "sort",
                "exactness",
            ])
            .await?;
        task.wait_for_completion(&self.client, None, Some(std::time::Duration::from_secs(300))).await?;

        info!(
            "Meilisearch index '{}' configured successfully",
            self.index_name
        );
        Ok(())
    }

    /// Bulk-add or replace documents in the index.
    pub async fn add_documents<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        documents: &[T],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if documents.is_empty() {
            return Ok(());
        }
        let index = self.client.index(&self.index_name);
        let task = index
            .add_documents(documents, Some(self.primary_key.as_str()))
            .await?;
        // Use custom wait_for_task which has no timeout — the SDK default of 5s
        // is too short when Meilisearch generates embeddings via a slow model.
        self.wait_for_task(task.task_uid as u64).await?;
        Ok(())
    }

    /// Add or update a single document (upsert).
    pub async fn upsert_document<T: Serialize + DeserializeOwned + Send + Sync>(
        &self,
        document: &T,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = self.client.index(&self.index_name);
        let task = index
            .add_documents(std::slice::from_ref(document), Some(self.primary_key.as_str()))
            .await?;
        self.wait_for_task(task.task_uid as u64).await?;
        Ok(())
    }

    /// Delete a document by its primary key value.
    pub async fn delete_document(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = self.client.index(&self.index_name);
        match index.delete_document(id).await {
            Ok(task) => {
                self.wait_for_task(task.task_uid as u64).await?;
            }
            Err(e) => {
                error!(
                    "Failed to delete document '{id}' from index '{}': {e}",
                    self.index_name
                );
            }
        }
        Ok(())
    }

    async fn configure_embedders_via_http(
        &self,
        embedder_config: &MeilisearchEmbedderConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut embedder = serde_json::Map::new();
        embedder.insert(
            "source".to_string(),
            serde_json::Value::String(embedder_config.source.clone()),
        );

        if let Some(url) = &embedder_config.url {
            embedder.insert("url".to_string(), serde_json::Value::String(url.clone()));
        }
        if let Some(api_key) = &embedder_config.api_key {
            embedder.insert("apiKey".to_string(), serde_json::Value::String(api_key.clone()));
        }
        if embedder_config.source != "rest" {
            if let Some(model) = &embedder_config.model {
                embedder.insert("model".to_string(), serde_json::Value::String(model.clone()));
            }
        }
        if let Some(revision) = &embedder_config.revision {
            embedder.insert(
                "revision".to_string(),
                serde_json::Value::String(revision.clone()),
            );
        }
        if let Some(pooling) = &embedder_config.pooling {
            embedder.insert(
                "pooling".to_string(),
                serde_json::Value::String(pooling.clone()),
            );
        }
        if let Some(document_template) = &embedder_config.document_template {
            embedder.insert(
                "documentTemplate".to_string(),
                serde_json::Value::String(document_template.clone()),
            );
        }
        if let Some(document_template_max_bytes) = embedder_config.document_template_max_bytes {
            embedder.insert(
                "documentTemplateMaxBytes".to_string(),
                serde_json::Value::Number(document_template_max_bytes.into()),
            );
        }
        if let Some(dimensions) = embedder_config.dimensions {
            embedder.insert(
                "dimensions".to_string(),
                serde_json::Value::Number(dimensions.into()),
            );
        }
        if let Some(request) = &embedder_config.request {
            embedder.insert("request".to_string(), request.clone());
        }
        if let Some(response) = &embedder_config.response {
            embedder.insert("response".to_string(), response.clone());
        }
        if let Some(headers) = &embedder_config.headers {
            embedder.insert("headers".to_string(), serde_json::to_value(headers)?);
        }
        if let Some(binary_quantized) = embedder_config.binary_quantized {
            embedder.insert(
                "binaryQuantized".to_string(),
                serde_json::Value::Bool(binary_quantized),
            );
        }
        if let Some(indexing_fragments) = &embedder_config.indexing_fragments {
            embedder.insert("indexingFragments".to_string(), indexing_fragments.clone());
        }
        if let Some(search_fragments) = &embedder_config.search_fragments {
            embedder.insert("searchFragments".to_string(), search_fragments.clone());
        }

        let payload = serde_json::json!({
            embedder_config.name.clone(): serde_json::Value::Object(embedder),
        });

        let url = format!(
            "{}/indexes/{}/settings/embedders",
            self.base_url, self.index_name
        );

        let mut request = self
            .http_client
            .patch(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        }

        let response = request.json(&payload).send().await?;
        let status = response.status();

        if status != StatusCode::ACCEPTED {
            let body = response.text().await?;
            return Err(format!(
                "Failed to configure embedders for index '{}': {} {}",
                self.index_name, status, body
            )
            .into());
        }

        let task: MeiliTaskInfo = response.json().await?;
        self.wait_for_task(task.task_uid).await?;
        Ok(())
    }

    async fn wait_for_task(
        &self,
        task_uid: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut poll_interval = std::time::Duration::from_millis(200);
        let max_interval = std::time::Duration::from_secs(5);
        loop {
            let url = format!("{}/tasks/{}", self.base_url, task_uid);
            let mut request = self.http_client.get(&url);

            if let Some(api_key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {api_key}"));
            }

            let response = request.send().await?;
            let status = response.status();
            if status != StatusCode::OK {
                let body = response.text().await?;
                return Err(format!(
                    "Failed to fetch Meilisearch task {}: {} {}",
                    task_uid, status, body
                )
                .into());
            }

            let task: MeiliTaskStatus = response.json().await?;
            match task.status.as_str() {
                "enqueued" | "processing" => {
                    tokio::time::sleep(poll_interval).await;
                    poll_interval = (poll_interval * 2).min(max_interval);
                }
                "succeeded" => return Ok(()),
                "failed" => {
                    return Err(task
                        .error
                        .map(|e| {
                            format!(
                                "Meilisearch task {} failed: {}",
                                task_uid, e.error_message
                            )
                        })
                        .unwrap_or_else(|| format!("Meilisearch task {} failed", task_uid))
                        .into())
                }
                other => {
                    return Err(format!(
                        "Unexpected Meilisearch task status for task {}: {}",
                        task_uid, other
                    )
                    .into())
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct MeiliTaskInfo {
    #[serde(rename = "taskUid")]
    task_uid: u64,
}

#[derive(Debug, Deserialize)]
struct MeiliTaskStatus {
    status: String,
    error: Option<MeiliTaskError>,
}

#[derive(Debug, Deserialize)]
struct MeiliTaskError {
    #[serde(rename = "message")]
    error_message: String,
}
