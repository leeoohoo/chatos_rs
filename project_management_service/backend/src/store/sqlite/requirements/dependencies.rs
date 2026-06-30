use std::collections::BTreeSet;

use super::super::super::common::normalize_id_list;
use super::super::super::sqlite_rows::requirement_dependency_from_row;
use super::super::SqliteStore;
use crate::models::*;

impl SqliteStore {
    pub async fn list_requirement_dependencies(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<RequirementDependencyRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM requirement_dependencies
             WHERE requirement_id = ?1
             ORDER BY created_at ASC",
        )
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(requirement_dependency_from_row).collect())
    }

    pub async fn set_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        self.validate_requirement_dependencies(requirement_id, &prerequisite_ids)
            .await?;
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM requirement_dependencies WHERE requirement_id = ?1")
            .bind(requirement_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        let now = now_rfc3339();
        for prerequisite_id in normalize_id_list(prerequisite_ids) {
            sqlx::query(
                "INSERT INTO requirement_dependencies (
                    requirement_id, prerequisite_requirement_id, relation_type, created_at
                 ) VALUES (?1, ?2, 'blocks', ?3)",
            )
            .bind(requirement_id)
            .bind(prerequisite_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn validate_requirement_dependencies(
        &self,
        requirement_id: &str,
        prerequisite_ids: &[String],
    ) -> Result<(), String> {
        if prerequisite_ids.len() > 50 {
            return Err("前置需求数量不能超过 50 个".to_string());
        }
        let requirement = self
            .get_requirement(requirement_id)
            .await?
            .ok_or_else(|| format!("需求不存在: {requirement_id}"))?;
        let prerequisite_ids = normalize_id_list(prerequisite_ids.to_vec());
        for prerequisite_id in &prerequisite_ids {
            if prerequisite_id == requirement_id {
                return Err("需求不能依赖自身".to_string());
            }
            let prerequisite = self
                .get_requirement(prerequisite_id)
                .await?
                .ok_or_else(|| format!("前置需求不存在: {prerequisite_id}"))?;
            if prerequisite.project_id != requirement.project_id {
                return Err(format!("前置需求必须属于同一项目: {prerequisite_id}"));
            }
            if matches!(
                prerequisite.status,
                RequirementStatus::Cancelled | RequirementStatus::Archived
            ) {
                return Err(format!(
                    "已取消或归档需求不能作为前置需求: {prerequisite_id}"
                ));
            }
        }
        self.ensure_requirement_dependency_acyclic(requirement_id, prerequisite_ids)
            .await
    }

    async fn ensure_requirement_dependency_acyclic(
        &self,
        requirement_id: &str,
        prerequisite_ids: Vec<String>,
    ) -> Result<(), String> {
        let mut stack = prerequisite_ids;
        let mut visited = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if current == requirement_id {
                return Err(format!("前置需求不能形成循环依赖: {requirement_id}"));
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if visited.len() > 200 {
                return Err("需求依赖链过深或过大，请拆分后再保存".to_string());
            }
            for edge in self.list_requirement_dependencies(&current).await? {
                stack.push(edge.prerequisite_requirement_id);
            }
        }
        Ok(())
    }
}
