use super::*;

impl AppStore {
    pub async fn list_skills(&self) -> Result<Vec<SkillRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_skills()),
            Self::Sqlite(store) => store.list_skills().await,
            Self::Mongo(store) => store.list_skills().await,
        }
    }

    pub async fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_skill(id)),
            Self::Sqlite(store) => store.get_skill(id).await,
            Self::Mongo(store) => store.get_skill(id).await,
        }
    }

    pub async fn save_skill(&self, skill: SkillRecord) -> Result<SkillRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_skill(skill)),
            Self::Sqlite(store) => store.save_skill(skill).await,
            Self::Mongo(store) => store.save_skill(skill).await,
        }
    }

    pub async fn delete_skill(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_skill(id)),
            Self::Sqlite(store) => store.delete_skill(id).await,
            Self::Mongo(store) => store.delete_skill(id).await,
        }
    }
}
