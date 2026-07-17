// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoConnectorStore {
    pub async fn create_managed_requirements_policy(
        &self,
        policy: &ManagedRequirementsPolicy,
    ) -> Result<(), String> {
        self.managed_requirements_policies
            .insert_one(policy, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_managed_requirements_policy(
        &self,
        id: &str,
    ) -> Result<Option<ManagedRequirementsPolicy>, String> {
        self.managed_requirements_policies
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_managed_requirements_policies(
        &self,
    ) -> Result<Vec<ManagedRequirementsPolicy>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "name": 1, "updated_at": -1 })
            .build();
        let cursor = self
            .managed_requirements_policies
            .find(None, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }

    pub async fn update_managed_requirements_policy(
        &self,
        policy: &ManagedRequirementsPolicy,
    ) -> Result<bool, String> {
        let result = self
            .managed_requirements_policies
            .update_one(
                doc! { "id": &policy.id },
                doc! {
                    "$set": {
                        "name": &policy.name,
                        "description": &policy.description,
                        "requirements_toml": &policy.requirements_toml,
                        "content_sha256": &policy.content_sha256,
                        "version": policy.version,
                        "enabled": policy.enabled,
                        "updated_by": &policy.updated_by,
                        "updated_at": &policy.updated_at,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
    }

    pub async fn delete_managed_requirements_policy(&self, id: &str) -> Result<bool, String> {
        self.managed_requirements_policies
            .delete_one(doc! { "id": id }, None)
            .await
            .map(|result| result.deleted_count == 1)
            .map_err(|err| err.to_string())
    }

    pub async fn managed_requirements_policy_has_assignments(
        &self,
        policy_id: &str,
    ) -> Result<bool, String> {
        self.managed_requirements_assignments
            .find_one(doc! { "policy_id": policy_id }, None)
            .await
            .map(|assignment| assignment.is_some())
            .map_err(|err| err.to_string())
    }

    pub async fn create_managed_requirements_assignment(
        &self,
        assignment: &ManagedRequirementsAssignment,
    ) -> Result<(), String> {
        self.managed_requirements_assignments
            .insert_one(assignment, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn get_managed_requirements_assignment(
        &self,
        id: &str,
    ) -> Result<Option<ManagedRequirementsAssignment>, String> {
        self.managed_requirements_assignments
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn list_managed_requirements_assignments(
        &self,
    ) -> Result<Vec<ManagedRequirementsAssignment>, String> {
        let options = FindOptions::builder()
            .sort(doc! { "scope": 1, "subject": 1, "priority": 1, "updated_at": -1 })
            .build();
        let cursor = self
            .managed_requirements_assignments
            .find(None, options)
            .await
            .map_err(|err| err.to_string())?;
        cursor.try_collect().await.map_err(|err| err.to_string())
    }

    pub async fn update_managed_requirements_assignment(
        &self,
        assignment: &ManagedRequirementsAssignment,
    ) -> Result<bool, String> {
        let result = self
            .managed_requirements_assignments
            .update_one(
                doc! { "id": &assignment.id },
                doc! {
                    "$set": {
                        "policy_id": &assignment.policy_id,
                        "scope": &assignment.scope,
                        "subject": &assignment.subject,
                        "priority": assignment.priority,
                        "enabled": assignment.enabled,
                        "updated_by": &assignment.updated_by,
                        "updated_at": &assignment.updated_at,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count == 1)
    }

    pub async fn delete_managed_requirements_assignment(&self, id: &str) -> Result<bool, String> {
        self.managed_requirements_assignments
            .delete_one(doc! { "id": id }, None)
            .await
            .map(|result| result.deleted_count == 1)
            .map_err(|err| err.to_string())
    }

    pub async fn applicable_managed_requirements_layers(
        &self,
        owner_user_id: &str,
        role: &str,
    ) -> Result<Vec<ApplicableManagedRequirementsLayer>, String> {
        let mut scopes = vec![
            doc! { "scope": MANAGED_REQUIREMENTS_SCOPE_GLOBAL },
            doc! { "scope": MANAGED_REQUIREMENTS_SCOPE_USER, "subject": owner_user_id },
        ];
        if !role.trim().is_empty() {
            scopes.push(doc! { "scope": MANAGED_REQUIREMENTS_SCOPE_ROLE, "subject": role });
        }
        let cursor = self
            .managed_requirements_assignments
            .find(doc! { "$or": scopes }, None)
            .await
            .map_err(|err| err.to_string())?;
        let assignments = cursor
            .try_collect::<Vec<ManagedRequirementsAssignment>>()
            .await
            .map_err(|err| err.to_string())?;
        if assignments.is_empty() {
            return Ok(Vec::new());
        }
        let policy_ids = assignments
            .iter()
            .map(|assignment| assignment.policy_id.clone())
            .collect::<Vec<_>>();
        let cursor = self
            .managed_requirements_policies
            .find(doc! { "id": { "$in": policy_ids } }, None)
            .await
            .map_err(|err| err.to_string())?;
        let policies = cursor
            .try_collect::<Vec<ManagedRequirementsPolicy>>()
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .map(|policy| (policy.id.clone(), policy))
            .collect::<HashMap<_, _>>();
        Ok(collect_applicable_managed_requirements_layers(
            assignments,
            policies,
        ))
    }
}

pub(super) fn collect_applicable_managed_requirements_layers(
    assignments: Vec<ManagedRequirementsAssignment>,
    policies: HashMap<String, ManagedRequirementsPolicy>,
) -> Vec<ApplicableManagedRequirementsLayer> {
    let mut layers = assignments
        .into_iter()
        .filter(|assignment| assignment.enabled)
        .filter_map(|assignment| {
            policies
                .get(assignment.policy_id.as_str())
                .filter(|policy| policy.enabled)
                .cloned()
                .map(|policy| ApplicableManagedRequirementsLayer { policy, assignment })
        })
        .collect::<Vec<_>>();
    layers.sort_by(|left, right| {
        managed_scope_rank(left.assignment.scope.as_str())
            .cmp(&managed_scope_rank(right.assignment.scope.as_str()))
            .then(left.assignment.priority.cmp(&right.assignment.priority))
            .then(left.assignment.updated_at.cmp(&right.assignment.updated_at))
            .then(left.assignment.id.cmp(&right.assignment.id))
    });
    layers
}

pub(super) fn managed_scope_rank(scope: &str) -> u8 {
    match scope {
        MANAGED_REQUIREMENTS_SCOPE_GLOBAL => 0,
        MANAGED_REQUIREMENTS_SCOPE_ROLE => 1,
        MANAGED_REQUIREMENTS_SCOPE_USER => 2,
        _ => u8::MAX,
    }
}
