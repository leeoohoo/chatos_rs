// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn list_skills(&self) -> Result<Vec<SkillRecord>, String> {
        self.load_collection_items_with_query(
            &self.skills,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    pub(in crate::store) async fn get_skill(
        &self,
        id: &str,
    ) -> Result<Option<SkillRecord>, String> {
        self.find_by_id(&self.skills, id).await
    }

    pub(in crate::store) async fn save_skill(
        &self,
        skill: SkillRecord,
    ) -> Result<SkillRecord, String> {
        self.upsert_by_id(&self.skills, &skill.id, &skill).await?;
        Ok(skill)
    }

    pub(in crate::store) async fn delete_skill(&self, id: &str) -> Result<bool, String> {
        self.delete_by_id(&self.skills, id).await
    }
}
