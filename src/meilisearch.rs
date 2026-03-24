use std::collections::HashMap;

use log::{error, info};
use meilisearch_sdk::{
    client::Client,
    settings::{Embedder, EmbedderSource},
};
use serde::{de::DeserializeOwned, Serialize};

use crate::config::MeilisearchEmbedderConfig;

pub struct MeiliIndex {
    client: Client,
    index_name: String,
    primary_key: String,
}

impl MeiliIndex {
    pub fn new(url: &str, key: Option<&str>, index_name: &str, primary_key: &str) -> Self {
        let client = Client::new(url, key).expect("Failed to create Meilisearch client");
        MeiliIndex {
            client,
            index_name: index_name.to_owned(),
            primary_key: primary_key.to_owned(),
        }
    }

    pub fn index_name(&self) -> &str {
        &self.index_name
    }

    /// Configure the "media" index with its specific settings.
    pub async fn setup_media_index(
        &self,
        embedder_config: &MeilisearchEmbedderConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Configuring Meilisearch index '{}'...", self.index_name);

        let task = self
            .client
            .create_index(&self.index_name, Some(&self.primary_key))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let index = self.client.index(&self.index_name);

        let task = index
            .set_searchable_attributes(["name", "description", "owner"])
            .await?;
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

        let embedders = HashMap::from([(
            embedder_config.name.clone(),
            Embedder {
                source: match embedder_config.source.as_str() {
                    "huggingFace" => EmbedderSource::HuggingFace,
                    "openAi" => EmbedderSource::OpenAi,
                    "ollama" => EmbedderSource::Ollama,
                    "rest" => EmbedderSource::Rest,
                    "composite" => EmbedderSource::Composite,
                    _ => EmbedderSource::UserProvided,
                },
                url: embedder_config.url.clone(),
                api_key: embedder_config.api_key.clone(),
                model: embedder_config.model.clone(),
                revision: embedder_config.revision.clone(),
                pooling: embedder_config.pooling.clone(),
                document_template: embedder_config.document_template.clone(),
                document_template_max_bytes: embedder_config.document_template_max_bytes,
                dimensions: embedder_config.dimensions,
                request: embedder_config.request.clone(),
                response: embedder_config.response.clone(),
                binary_quantized: embedder_config.binary_quantized,
                ..Embedder::default()
            },
        )]);
        let task = index.set_embedders(&embedders).await?;
        task.wait_for_completion(&self.client, None, None).await?;

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
