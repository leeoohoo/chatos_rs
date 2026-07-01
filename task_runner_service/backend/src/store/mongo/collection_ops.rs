// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(super) async fn load_collection_items_with_query<T>(
        &self,
        collection: &Collection<T>,
        filter: Document,
        options: Option<FindOptions>,
    ) -> Result<Vec<T>, String>
    where
        T: DeserializeOwned + Unpin + Send + Sync,
    {
        let mut cursor = collection
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        while let Some(item) = cursor.try_next().await.map_err(|err| err.to_string())? {
            items.push(item);
        }
        Ok(items)
    }

    pub(super) async fn aggregate_documents<T>(
        &self,
        collection: &Collection<T>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<Document>, String>
    where
        T: Send + Sync,
    {
        let mut cursor = collection
            .aggregate(pipeline, None)
            .await
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        while let Some(item) = cursor.try_next().await.map_err(|err| err.to_string())? {
            items.push(item);
        }
        Ok(items)
    }

    pub(super) async fn aggregate_collection_items<T>(
        &self,
        collection: &Collection<T>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<T>, String>
    where
        T: DeserializeOwned + Send + Sync,
    {
        self.aggregate_documents(collection, pipeline)
            .await?
            .into_iter()
            .map(|doc| bson::from_document(doc).map_err(|err| err.to_string()))
            .collect()
    }

    pub(super) async fn aggregate_into_items<S, T>(
        &self,
        collection: &Collection<S>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<T>, String>
    where
        S: Send + Sync,
        T: DeserializeOwned,
    {
        self.aggregate_documents(collection, pipeline)
            .await?
            .into_iter()
            .map(|doc| bson::from_document(doc).map_err(|err| err.to_string()))
            .collect()
    }

    pub(super) async fn find_by_id<T>(
        &self,
        collection: &Collection<T>,
        id: &str,
    ) -> Result<Option<T>, String>
    where
        T: DeserializeOwned + Unpin + Send + Sync,
    {
        collection
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub(super) async fn upsert_by_id<T>(
        &self,
        collection: &Collection<T>,
        id: &str,
        value: &T,
    ) -> Result<(), String>
    where
        T: Serialize + Send + Sync,
    {
        collection
            .replace_one(
                doc! { "id": id },
                value,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub(super) async fn delete_by_id<T>(
        &self,
        collection: &Collection<T>,
        id: &str,
    ) -> Result<bool, String>
    where
        T: Send + Sync,
    {
        collection
            .delete_one(doc! { "id": id }, None)
            .await
            .map(|result| result.deleted_count > 0)
            .map_err(|err| err.to_string())
    }
}
