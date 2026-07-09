// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use super::{upsert_one, MongoStore};
use crate::models::*;

impl MongoStore {
    pub async fn get_project_runtime_environment(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectRuntimeEnvironmentRecord>, String> {
        self.runtime_environments
            .find_one(doc! { "project_id": project_id.trim() }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn upsert_project_runtime_environment(
        &self,
        environment: &ProjectRuntimeEnvironmentRecord,
    ) -> Result<ProjectRuntimeEnvironmentRecord, String> {
        upsert_one(
            &self.runtime_environments,
            doc! { "project_id": environment.project_id.as_str() },
            environment,
        )
        .await?;
        Ok(environment.clone())
    }

    pub async fn list_project_runtime_environment_images(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectRuntimeEnvironmentImageRecord>, String> {
        super::find_many(
            &self.runtime_environment_images,
            doc! { "project_id": project_id.trim() },
            Some(doc! { "environment_key": 1, "id": 1 }),
        )
        .await
    }

    pub async fn replace_project_runtime_environment_images(
        &self,
        project_id: &str,
        images: &[ProjectRuntimeEnvironmentImageRecord],
    ) -> Result<Vec<ProjectRuntimeEnvironmentImageRecord>, String> {
        self.runtime_environment_images
            .delete_many(doc! { "project_id": project_id.trim() }, None)
            .await
            .map_err(|err| err.to_string())?;
        for image in images {
            upsert_one(
                &self.runtime_environment_images,
                doc! { "id": image.id.as_str() },
                image,
            )
            .await?;
        }
        Ok(images.to_vec())
    }
}
