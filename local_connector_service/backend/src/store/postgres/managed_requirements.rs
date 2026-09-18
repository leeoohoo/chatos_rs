// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use crate::models::{
    ApplicableManagedRequirementsLayer, ManagedRequirementsAssignment, ManagedRequirementsPolicy,
    MANAGED_REQUIREMENTS_SCOPE_GLOBAL, MANAGED_REQUIREMENTS_SCOPE_ROLE,
    MANAGED_REQUIREMENTS_SCOPE_USER,
};

use super::super::{db_error, decode_all, decode_optional, json, timestamp, ConnectorStore};

impl ConnectorStore {
    pub async fn create_managed_requirements_policy(
        &self,
        policy: &ManagedRequirementsPolicy,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_managed_requirements_policies(id,name,enabled,updated_at,data) VALUES($1,$2,$3,$4,$5)")
            .bind(&policy.id).bind(&policy.name).bind(policy.enabled).bind(timestamp(&policy.updated_at)?)
            .bind(json(policy)?).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn get_managed_requirements_policy(
        &self,
        id: &str,
    ) -> Result<Option<ManagedRequirementsPolicy>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM local_connector_managed_requirements_policies WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn list_managed_requirements_policies(
        &self,
    ) -> Result<Vec<ManagedRequirementsPolicy>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM local_connector_managed_requirements_policies ORDER BY name,updated_at DESC")
            .fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn update_managed_requirements_policy(
        &self,
        policy: &ManagedRequirementsPolicy,
    ) -> Result<bool, String> {
        sqlx::query("UPDATE local_connector_managed_requirements_policies SET name=$2,enabled=$3,updated_at=$4,data=$5 WHERE id=$1")
            .bind(&policy.id).bind(&policy.name).bind(policy.enabled).bind(timestamp(&policy.updated_at)?)
            .bind(json(policy)?).execute(&self.pool).await.map(|result| result.rows_affected()==1).map_err(db_error)
    }

    pub async fn delete_managed_requirements_policy(&self, id: &str) -> Result<bool, String> {
        sqlx::query("DELETE FROM local_connector_managed_requirements_policies WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
    }

    pub async fn managed_requirements_policy_has_assignments(
        &self,
        policy_id: &str,
    ) -> Result<bool, String> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM local_connector_managed_requirements_assignments WHERE policy_id=$1)")
            .bind(policy_id).fetch_one(&self.pool).await.map_err(db_error)
    }

    pub async fn create_managed_requirements_assignment(
        &self,
        assignment: &ManagedRequirementsAssignment,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO local_connector_managed_requirements_assignments(id,policy_id,scope,subject,priority,enabled,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(&assignment.id).bind(&assignment.policy_id).bind(&assignment.scope).bind(&assignment.subject)
            .bind(assignment.priority).bind(assignment.enabled).bind(timestamp(&assignment.updated_at)?)
            .bind(json(assignment)?).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn get_managed_requirements_assignment(
        &self,
        id: &str,
    ) -> Result<Option<ManagedRequirementsAssignment>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM local_connector_managed_requirements_assignments WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn list_managed_requirements_assignments(
        &self,
    ) -> Result<Vec<ManagedRequirementsAssignment>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM local_connector_managed_requirements_assignments ORDER BY scope,subject NULLS FIRST,priority,updated_at DESC")
            .fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn update_managed_requirements_assignment(
        &self,
        assignment: &ManagedRequirementsAssignment,
    ) -> Result<bool, String> {
        sqlx::query("UPDATE local_connector_managed_requirements_assignments SET policy_id=$2,scope=$3,subject=$4,priority=$5,enabled=$6,updated_at=$7,data=$8 WHERE id=$1")
            .bind(&assignment.id).bind(&assignment.policy_id).bind(&assignment.scope).bind(&assignment.subject)
            .bind(assignment.priority).bind(assignment.enabled).bind(timestamp(&assignment.updated_at)?)
            .bind(json(assignment)?).execute(&self.pool).await.map(|result| result.rows_affected()==1).map_err(db_error)
    }

    pub async fn delete_managed_requirements_assignment(&self, id: &str) -> Result<bool, String> {
        sqlx::query("DELETE FROM local_connector_managed_requirements_assignments WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
    }

    pub async fn applicable_managed_requirements_layers(
        &self,
        owner_user_id: &str,
        role: &str,
    ) -> Result<Vec<ApplicableManagedRequirementsLayer>, String> {
        let assignments: Vec<ManagedRequirementsAssignment> = decode_all(
            sqlx::query_scalar(
                "SELECT data FROM local_connector_managed_requirements_assignments WHERE
             (scope=$1) OR (scope=$2 AND subject=$3) OR ($4<>'' AND scope=$5 AND subject=$4)",
            )
            .bind(MANAGED_REQUIREMENTS_SCOPE_GLOBAL)
            .bind(MANAGED_REQUIREMENTS_SCOPE_USER)
            .bind(owner_user_id)
            .bind(role.trim())
            .bind(MANAGED_REQUIREMENTS_SCOPE_ROLE)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )?;
        if assignments.is_empty() {
            return Ok(Vec::new());
        }
        let policy_ids = assignments
            .iter()
            .map(|item| item.policy_id.clone())
            .collect::<Vec<_>>();
        let policies: Vec<ManagedRequirementsPolicy> = decode_all(
            sqlx::query_scalar(
                "SELECT data FROM local_connector_managed_requirements_policies WHERE id=ANY($1)",
            )
            .bind(policy_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )?;
        Ok(collect_applicable_managed_requirements_layers(
            assignments,
            policies
                .into_iter()
                .map(|policy| (policy.id.clone(), policy))
                .collect(),
        ))
    }
}

fn collect_applicable_managed_requirements_layers(
    assignments: Vec<ManagedRequirementsAssignment>,
    policies: HashMap<String, ManagedRequirementsPolicy>,
) -> Vec<ApplicableManagedRequirementsLayer> {
    let mut layers = assignments
        .into_iter()
        .filter(|assignment| assignment.enabled)
        .filter_map(|assignment| {
            policies
                .get(&assignment.policy_id)
                .filter(|policy| policy.enabled)
                .cloned()
                .map(|policy| ApplicableManagedRequirementsLayer { policy, assignment })
        })
        .collect::<Vec<_>>();
    layers.sort_by(|left, right| {
        managed_scope_rank(&left.assignment.scope)
            .cmp(&managed_scope_rank(&right.assignment.scope))
            .then(left.assignment.priority.cmp(&right.assignment.priority))
            .then(left.assignment.updated_at.cmp(&right.assignment.updated_at))
            .then(left.assignment.id.cmp(&right.assignment.id))
    });
    layers
}

fn managed_scope_rank(scope: &str) -> u8 {
    match scope {
        MANAGED_REQUIREMENTS_SCOPE_GLOBAL => 0,
        MANAGED_REQUIREMENTS_SCOPE_ROLE => 1,
        MANAGED_REQUIREMENTS_SCOPE_USER => 2,
        _ => u8::MAX,
    }
}
