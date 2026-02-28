use log::{error, info};
use meilisearch_sdk::client::Client;
use meilisearch_sdk::indexes::Index;

use crate::model::MeiliMedia;

const INDEX_NAME: &str = "media";

pub struct MeiliIndex {
    client: Client,
}

impl MeiliIndex {
    pub fn new(url: &str, key: Option<&str>) -> Self {
        let client = Client::new(url, key).expect("Failed to create Meilisearch client");
        MeiliIndex { client }
    }

    /// Create or configure the "media" index with settings from MEILISEARCH_SCHEMA.md.
    pub async fn setup_index(&self) -> Result<Index, Box<dyn std::error::Error>> {
        info!("Configuring Meilisearch index '{INDEX_NAME}'...");

        // Create index with primary key (idempotent — will use existing if present)
        let task = self
            .client
            .create_index(INDEX_NAME, Some("id"))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        let index = self.client.index(INDEX_NAME);

        // Searchable attributes — name is primary, owner allows searching by creator
        let task = index
            .set_searchable_attributes(["name", "owner"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        // Filterable attributes for search filters
        let task = index
            .set_filterable_attributes(["type", "upload", "views", "likes", "visibility", "restricted_to_group"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        // Sortable attributes for sort options
        let task = index
            .set_sortable_attributes(["upload", "views", "likes"])
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;

        // Ranking rules (default Meilisearch ranking)
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

        info!("Meilisearch index '{INDEX_NAME}' configured successfully");
        Ok(index)
    }

    /// Bulk-add or replace documents in the index.
    pub async fn add_documents(
        &self,
        documents: &[MeiliMedia],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if documents.is_empty() {
            return Ok(());
        }
        let index = self.client.index(INDEX_NAME);
        let task = index.add_documents(documents, Some("id")).await?;
        task.wait_for_completion(&self.client, None, None).await?;
        Ok(())
    }

    /// Add or update a single document (upsert).
    pub async fn upsert_document(
        &self,
        document: &MeiliMedia,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let index = self.client.index(INDEX_NAME);
        let task = index
            .add_documents(&[document.clone()], Some("id"))
            .await?;
        task.wait_for_completion(&self.client, None, None).await?;
        Ok(())
    }

    /// Delete a document by its media ID.
    pub async fn delete_document(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let index = self.client.index(INDEX_NAME);
        match index.delete_document(id).await {
            Ok(task) => {
                task.wait_for_completion(&self.client, None, None).await?;
            }
            Err(e) => {
                error!("Failed to delete document '{id}' from Meilisearch: {e}");
            }
        }
        Ok(())
    }
}
