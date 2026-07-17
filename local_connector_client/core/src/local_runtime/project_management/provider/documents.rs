// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_mcp_contract::args::{
    ListRequirementTechnicalDocumentsArgs, RequirementTechnicalDocumentIdArgs,
    UpsertRequirementTechnicalDocumentArgs,
};
use serde_json::{json, Value};

use crate::local_runtime::project_management::UpsertLocalRequirementDocumentInput;

use super::requirement_support::require_mutable;
use super::{decode, normalized, LocalProjectManagementProvider};

pub(super) async fn list(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListRequirementTechnicalDocumentsArgs = decode(arguments)?;
    require_requirement(provider, args.requirement_id.as_str()).await?;
    let doc_type = normalized(args.doc_type);
    let records = provider
        .database
        .list_local_requirement_documents(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            args.requirement_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|record| {
            doc_type
                .as_deref()
                .is_none_or(|value| record.doc_type == value)
        })
        .collect::<Vec<_>>();
    let total = records.len();
    Ok(json!({ "items": records, "total": total }))
}

pub(super) async fn get(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: RequirementTechnicalDocumentIdArgs = decode(arguments)?;
    require_requirement(provider, args.requirement_id.as_str()).await?;
    let record = provider
        .database
        .get_local_requirement_document(
            provider.owner_user_id.as_str(),
            args.requirement_id.as_str(),
            args.document_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local requirement document was not found".to_string())?;
    serde_json::to_value(record).map_err(|error| error.to_string())
}

pub(super) async fn upsert(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpsertRequirementTechnicalDocumentArgs = decode(arguments)?;
    require_mutable(provider, args.requirement_id.as_str()).await?;
    let doc_type = normalized(args.doc_type).unwrap_or_else(|| "technical_overview".to_string());
    let record = provider
        .database
        .upsert_local_requirement_document(UpsertLocalRequirementDocumentInput {
            document_id: normalized(args.document_id),
            requirement_id: args.requirement_id,
            owner_user_id: provider.owner_user_id.clone(),
            title: normalized(args.title).unwrap_or_else(|| default_title(doc_type.as_str())),
            format: normalized(args.format).unwrap_or_else(|| "markdown".to_string()),
            doc_type,
            content: required_content(args.content)?,
        })
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_value(record).map_err(|error| error.to_string())
}

async fn require_requirement(
    provider: &LocalProjectManagementProvider,
    requirement_id: &str,
) -> Result<(), String> {
    let record = provider
        .database
        .get_local_requirement(provider.owner_user_id.as_str(), requirement_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local requirement was not found".to_string())?;
    if record.project_id != provider.project_id || record.archived_at.is_some() {
        return Err("local requirement was not found".to_string());
    }
    Ok(())
}

fn required_content(value: String) -> Result<String, String> {
    (!value.trim().is_empty())
        .then_some(value)
        .ok_or_else(|| "content is required".to_string())
}

fn default_title(doc_type: &str) -> String {
    match doc_type {
        "technical_overview" => "实现技术总体文档",
        "implementation_plan" => "实现方案",
        "architecture_diagram" => "架构图",
        "api_design" => "接口设计",
        "data_model" => "数据模型",
        _ => "技术文档",
    }
    .to_string()
}
