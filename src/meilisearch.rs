use log::{error, info};
use meilisearch_sdk::client::Client;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;

pub struct MeiliIndex {
    client: Client,
    index_name: String,
    primary_key: String,
    /// llama.cpp embeddings endpoint URL, set via `with_embedder`.
    llama_cpp_url: Option<String>,
    /// Meilisearch embedder name to configure (default: "default").
    embedder_name: String,
    /// Number of embedding vector dimensions.
    embedding_dimensions: Option<usize>,
}

impl MeiliIndex {
    pub fn new(url: &str, key: Option<&str>, index_name: &str, primary_key: &str) -> Self {
        let client = Client::new(url, key).expect("Failed to create Meilisearch client");
        MeiliIndex {
            client,
            index_name: index_name.to_owned(),
            primary_key: primary_key.to_owned(),
            llama_cpp_url: None,
            embedder_name: "default".to_string(),
            embedding_dimensions: None,
        }
    }

    /// Configure a llama.cpp REST embedder to be set up when `setup_media_index` is called.
    pub fn with_embedder(mut self, url: &str, name: &str, dimensions: Option<usize>) -> Self {
        self.llama_cpp_url = Some(url.to_string());
        self.embedder_name = name.to_string();
        self.embedding_dimensions = dimensions;
        self
    }

    pub fn index_name(&self) -> &str {
        &self.index_name
    }

    /// Configure the "media" index with its specific settings.
    pub async fn setup_media_index(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        let task = self
            .client
            .create_index(&self.index_name, Some(&self.primary_key))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let index = self.client.index(&self.index_name);

        let task = index.set_searchable_attributes(["name", "owner"]).await?;
        task.wait_for_completion(&self.client, None, None).await?;

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
        task.wait_for_completion(&self.client, None, None).await?;

        let task = index
            .set_sortable_attributes(["upload", "views", "likes"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

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
        task.wait_for_completion(&self.client, None, None).await?;

        // Configure the llama.cpp REST embedder so Meilisearch can embed each
        // document automatically and power SimilarQuery in the main platform.
        if let Some(ref llama_url) = self.llama_cpp_url {
            info!(
                "Configuring Meilisearch embedder '{}' pointing to llama.cpp at {}...",
                self.embedder_name, llama_url
            );

            let mut embedder_config = serde_json::json!({
                "source": "rest",
                "url": llama_url,
                // llama.cpp exposes an OpenAI-compatible /v1/embeddings endpoint.
                "request": {"input": "{{text}}"},
                "response": {"data": [{"embedding": "{{embedding}}"}]},
                // Embed the media title for each document.
                "documentTemplate": "{{doc.name}}"
            });

            if let Some(dims) = self.embedding_dimensions {
                embedder_config["dimensions"] = serde_json::json!(dims);
            }

            let mut embedders: HashMap<String, serde_json::Value> = HashMap::new();
            embedders.insert(self.embedder_name.clone(), embedder_config);

            let task = index.set_embedders(embedders).await?;
            task.wait_for_completion(&self.client, None, None).await?;

            info!("Embedder '{}' configured successfully", self.embedder_name);
        }

        info!(
            "Meilisearch index '{}' configured successfully",
            self.index_name
        );
        Ok(())
    }

    /// Configure the "lists" index.
    pub async fn setup_lists_index(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        let task = self
            .client
            .create_index(&self.index_name, Some(&self.primary_key))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let index = self.client.index(&self.index_name);

        let task = index.set_searchable_attributes(["name", "owner"]).await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let task = index
            .set_filterable_attributes(["visibility", "restricted_to_group", "owner"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let task = index
            .set_sortable_attributes(["created", "item_count"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

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
        task.wait_for_completion(&self.client, None, None).await?;

        info!(
            "Meilisearch index '{}' configured successfully",
            self.index_name
        );
        Ok(())
    }

    /// Configure the "users" index.
    pub async fn setup_users_index(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        let task = self
            .client
            .create_index(&self.index_name, Some(&self.primary_key))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let index = self.client.index(&self.index_name);

        let task = index
            .set_searchable_attributes(["name", "login"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

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
        task.wait_for_completion(&self.client, None, None).await?;

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
        task.wait_for_completion(&self.client, None, None).await?;
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
        task.wait_for_completion(&self.client, None, None).await?;
        Ok(())
    }

    /// Delete a document by its primary key value.
    pub async fn delete_document(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = self.client.index(&self.index_name);
        match index.delete_document(id).await {
            Ok(task) => {
                task.wait_for_completion(&self.client, None, None).await?;
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
}
