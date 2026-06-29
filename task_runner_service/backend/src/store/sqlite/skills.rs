use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_skills(&self) -> Result<Vec<SkillRecord>, String> {
        let rows = sqlx::query("SELECT * FROM skills ORDER BY datetime(updated_at) DESC, id DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(skill_from_row).collect()
    }

    pub(in crate::store) async fn get_skill(
        &self,
        id: &str,
    ) -> Result<Option<SkillRecord>, String> {
        let row = sqlx::query("SELECT * FROM skills WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(skill_from_row).transpose()
    }

    pub(in crate::store) async fn save_skill(
        &self,
        skill: SkillRecord,
    ) -> Result<SkillRecord, String> {
        sqlx::query(
            "INSERT INTO skills (
                id, name, display_name, description, content, locale, tags_json,
                source, source_url, source_registry, source_package_id, version, checksum,
                package_root, package_manifest_json, package_file_count, package_total_bytes,
                source_repo, source_ref, source_path,
                install_status, enabled, auto_inject, scope,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                installed_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                display_name = excluded.display_name,
                description = excluded.description,
                content = excluded.content,
                locale = excluded.locale,
                tags_json = excluded.tags_json,
                source = excluded.source,
                source_url = excluded.source_url,
                source_registry = excluded.source_registry,
                source_package_id = excluded.source_package_id,
                version = excluded.version,
                checksum = excluded.checksum,
                package_root = excluded.package_root,
                package_manifest_json = excluded.package_manifest_json,
                package_file_count = excluded.package_file_count,
                package_total_bytes = excluded.package_total_bytes,
                source_repo = excluded.source_repo,
                source_ref = excluded.source_ref,
                source_path = excluded.source_path,
                install_status = excluded.install_status,
                enabled = excluded.enabled,
                auto_inject = excluded.auto_inject,
                scope = excluded.scope,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                installed_at = excluded.installed_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&skill.id)
        .bind(&skill.name)
        .bind(&skill.display_name)
        .bind(skill.description.clone())
        .bind(&skill.content)
        .bind(&skill.locale)
        .bind(encode_json(&skill.tags)?)
        .bind(skill.source.as_str())
        .bind(skill.source_url.clone())
        .bind(skill.source_registry.clone())
        .bind(skill.source_package_id.clone())
        .bind(skill.version.clone())
        .bind(skill.checksum.clone())
        .bind(skill.package_root.clone())
        .bind(encode_json(&skill.package_manifest)?)
        .bind(skill.package_file_count as i64)
        .bind(skill.package_total_bytes as i64)
        .bind(skill.source_repo.clone())
        .bind(skill.source_ref.clone())
        .bind(skill.source_path.clone())
        .bind(skill.install_status.as_str())
        .bind(bool_to_int(skill.enabled))
        .bind(bool_to_int(skill.auto_inject))
        .bind(skill.scope.as_str())
        .bind(skill.creator_user_id.clone())
        .bind(skill.creator_username.clone())
        .bind(skill.creator_display_name.clone())
        .bind(skill.owner_user_id.clone())
        .bind(skill.owner_username.clone())
        .bind(skill.owner_display_name.clone())
        .bind(skill.installed_at.clone())
        .bind(&skill.created_at)
        .bind(&skill.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(skill)
    }

    pub(in crate::store) async fn delete_skill(&self, id: &str) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM skills WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
