use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_skills(&self) -> Vec<SkillRecord> {
        let data = self.inner.read();
        let mut items = data.skills.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_skill(&self, id: &str) -> Option<SkillRecord> {
        self.inner.read().skills.get(id).cloned()
    }

    pub(in crate::store) fn save_skill(&self, skill: SkillRecord) -> SkillRecord {
        let mut data = self.inner.write();
        data.skills.insert(skill.id.clone(), skill.clone());
        skill
    }

    pub(in crate::store) fn delete_skill(&self, id: &str) -> bool {
        self.inner.write().skills.remove(id).is_some()
    }
}
