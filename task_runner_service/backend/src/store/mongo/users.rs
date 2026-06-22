use super::*;

impl MongoStore {
    pub(in crate::store) async fn count_users(&self) -> Result<i64, String> {
        self.users
            .count_documents(doc! {}, None)
            .await
            .map(|count| count as i64)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        self.load_collection_items_with_query(
            &self.users,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "username": 1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        self.find_by_id(&self.users, id).await
    }

    pub(in crate::store) async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        self.users
            .find_one(doc! { "username": username }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        self.upsert_by_id(&self.users, &user.id, &user).await?;
        Ok(user)
    }

    pub(in crate::store) async fn delete_user(&self, id: &str) -> Result<bool, String> {
        self.delete_by_id(&self.users, id).await
    }
}
