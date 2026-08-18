// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{Duration, Utc};
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, UpdateOptions};

use crate::models::{now_rfc3339, ProjectExecutionIntegrationRecord};

use super::MongoStore;

impl MongoStore {
    pub async fn get_execution_integration(
        &self,
        project_id: &str,
        execution_group_id: &str,
    ) -> Result<Option<ProjectExecutionIntegrationRecord>, String> {
        self.execution_integrations
            .find_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn ensure_execution_integration(
        &self,
        project_id: &str,
        execution_group_id: &str,
        target_branch: &str,
        execution_branch_ref: &str,
        initial_base_commit: &str,
        current_head_commit: &str,
    ) -> Result<ProjectExecutionIntegrationRecord, String> {
        let now = now_rfc3339();
        self.execution_integrations
            .update_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                },
                doc! {
                    "$setOnInsert": {
                        "id": format!("{project_id}:{execution_group_id}"),
                        "project_id": project_id,
                        "execution_group_id": execution_group_id,
                        "target_branch": target_branch,
                        "execution_branch_ref": execution_branch_ref,
                        "initial_base_commit": initial_base_commit,
                        "current_head_commit": current_head_commit,
                        "status": "active",
                        "lock_version": 0_i64,
                        "created_at": now.as_str(),
                        "updated_at": now.as_str(),
                    }
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let record = self
            .execution_integrations
            .find_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "execution integration disappeared after upsert".to_string())?;
        if record.target_branch != target_branch
            || record.execution_branch_ref != execution_branch_ref
        {
            return Err(
                "execution integration identity does not match the existing record".to_string(),
            );
        }
        Ok(record)
    }

    pub async fn acquire_execution_integration_lease(
        &self,
        project_id: &str,
        execution_group_id: &str,
        worker_id: &str,
        lease_token: &str,
        lease_seconds: i64,
    ) -> Result<Option<ProjectExecutionIntegrationRecord>, String> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let lease_until = (now + Duration::seconds(lease_seconds.max(5))).to_rfc3339();
        self.execution_integrations
            .find_one_and_update(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                    "status": { "$in": ["active", "blocked", "ready_to_promote", "failed"] },
                    "$or": [
                        { "lease_token": { "$exists": false } },
                        { "lease_token": null },
                        { "lease_until": { "$lte": now_text.as_str() } },
                        { "lease_token": lease_token },
                    ],
                },
                doc! {
                    "$set": {
                        "lock_owner_worker_id": worker_id,
                        "lease_token": lease_token,
                        "lease_until": lease_until,
                        "updated_at": now_text,
                    },
                    "$inc": { "lock_version": 1_i64 },
                },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn update_execution_integration_head(
        &self,
        project_id: &str,
        execution_group_id: &str,
        lease_token: &str,
        expected_head: &str,
        current_head: &str,
    ) -> Result<ProjectExecutionIntegrationRecord, String> {
        self.execution_integrations
            .find_one_and_update(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                    "lease_token": lease_token,
                    "current_head_commit": expected_head,
                },
                doc! {
                    "$set": {
                        "current_head_commit": current_head,
                        "status": "active",
                        "updated_at": now_rfc3339(),
                    }
                },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "execution integration CAS update failed".to_string())
    }

    pub async fn mark_execution_integration_blocked(
        &self,
        project_id: &str,
        execution_group_id: &str,
        lease_token: &str,
    ) -> Result<bool, String> {
        self.execution_integrations
            .update_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                    "lease_token": lease_token,
                },
                doc! {
                    "$set": {
                        "status": "blocked",
                        "updated_at": now_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|error| error.to_string())
    }

    pub async fn release_execution_integration_lease(
        &self,
        project_id: &str,
        execution_group_id: &str,
        lease_token: &str,
    ) -> Result<(), String> {
        self.execution_integrations
            .update_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                    "lease_token": lease_token,
                },
                doc! {
                    "$set": { "updated_at": now_rfc3339() },
                    "$unset": {
                        "lock_owner_worker_id": "",
                        "lease_token": "",
                        "lease_until": "",
                    }
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn acquire_branch_promotion_lease(
        &self,
        project_id: &str,
        target_branch: &str,
        worker_id: &str,
        lease_token: &str,
        lease_seconds: i64,
    ) -> Result<bool, String> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let lease_until = (now + Duration::seconds(lease_seconds.max(5))).to_rfc3339();
        self.branch_promotion_leases
            .update_one(
                doc! {
                    "project_id": project_id,
                    "target_branch": target_branch,
                },
                doc! {
                    "$setOnInsert": {
                        "id": format!("{project_id}:{target_branch}"),
                        "project_id": project_id,
                        "target_branch": target_branch,
                        "lock_version": 0_i64,
                        "created_at": now_text.as_str(),
                        "updated_at": now_text.as_str(),
                    },
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.branch_promotion_leases
            .update_one(
                doc! {
                    "project_id": project_id,
                    "target_branch": target_branch,
                    "$or": [
                        { "lease_token": { "$exists": false } },
                        { "lease_token": null },
                        { "lease_until": { "$lte": now_text.as_str() } },
                        { "lease_token": lease_token },
                    ],
                },
                doc! {
                    "$set": {
                        "owner_worker_id": worker_id,
                        "lease_token": lease_token,
                        "lease_until": lease_until,
                        "updated_at": now_text,
                    },
                    "$inc": { "lock_version": 1_i64 },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|error| error.to_string())
    }

    pub async fn release_branch_promotion_lease(
        &self,
        project_id: &str,
        target_branch: &str,
        lease_token: &str,
    ) -> Result<(), String> {
        self.branch_promotion_leases
            .update_one(
                doc! {
                    "project_id": project_id,
                    "target_branch": target_branch,
                    "lease_token": lease_token,
                },
                doc! {
                    "$set": { "updated_at": now_rfc3339() },
                    "$unset": {
                        "owner_worker_id": "",
                        "lease_token": "",
                        "lease_until": "",
                    }
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn mark_execution_integration_promoted(
        &self,
        project_id: &str,
        execution_group_id: &str,
        lease_token: &str,
        promoted_commit: &str,
    ) -> Result<bool, String> {
        let now = now_rfc3339();
        self.execution_integrations
            .update_one(
                doc! {
                    "project_id": project_id,
                    "execution_group_id": execution_group_id,
                    "lease_token": lease_token,
                },
                doc! {
                    "$set": {
                        "status": "promoted",
                        "promoted_commit": promoted_commit,
                        "completed_at": now.as_str(),
                        "updated_at": now,
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|error| error.to_string())
    }
}
