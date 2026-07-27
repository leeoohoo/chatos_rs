// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{self, doc, Document};
use mongodb::options::UpdateOptions;

use super::{upsert_one, MongoStore};
use crate::models::*;

impl MongoStore {
    pub async fn get_project_runtime_environment(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectRuntimeEnvironmentRecord>, String> {
        self.runtime_environments
            .clone_with_type::<Document>()
            .find_one(doc! { "project_id": project_id.trim() }, None)
            .await
            .map_err(|err| err.to_string())?
            .map(normalize_runtime_environment_document)
            .transpose()
    }

    pub async fn upsert_project_runtime_environment(
        &self,
        environment: &ProjectRuntimeEnvironmentRecord,
    ) -> Result<ProjectRuntimeEnvironmentRecord, String> {
        let document = bson::to_document(environment).map_err(|err| err.to_string())?;
        self.runtime_environments
            .update_one(
                doc! { "project_id": environment.project_id.as_str() },
                doc! {
                    "$set": document,
                    "$unset": { "primary_service_id": "" },
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
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

fn normalize_runtime_environment_document(
    mut document: Document,
) -> Result<ProjectRuntimeEnvironmentRecord, String> {
    if document.contains_key("execution_service_id") {
        document.remove("primary_service_id");
    } else if let Some(legacy) = document.remove("primary_service_id") {
        document.insert("execution_service_id", legacy);
    }
    bson::from_document(document).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_environment_document() -> Document {
        doc! {
            "project_id": "project-1",
            "status": "pending_image_build",
            "sandbox_enabled": true,
            "sandbox_provider": "cloud_sandbox_manager",
            "file_provider": "harness",
            "analysis_summary": null,
            "not_runnable_reason": null,
            "detected_stack": {},
            "required_services": [],
            "env_vars": {},
            "environment_variables": [],
            "generated_config_files": [],
            "last_agent_run_id": null,
            "last_error": null,
            "created_at": "now",
            "updated_at": "now",
        }
    }

    #[test]
    fn runtime_environment_document_prefers_new_execution_service_field() {
        let mut document = runtime_environment_document();
        document.insert("primary_service_id", "legacy-api");
        document.insert("execution_service_id", "workspace");

        let environment = normalize_runtime_environment_document(document)
            .expect("deserialize environment with both service fields");
        assert_eq!(
            environment.execution_service_id.as_deref(),
            Some("workspace")
        );
    }

    #[test]
    fn runtime_environment_document_migrates_legacy_execution_service_field() {
        let mut document = runtime_environment_document();
        document.insert("primary_service_id", "legacy-api");

        let environment = normalize_runtime_environment_document(document)
            .expect("deserialize environment with legacy service field");
        assert_eq!(
            environment.execution_service_id.as_deref(),
            Some("legacy-api")
        );
    }
}
