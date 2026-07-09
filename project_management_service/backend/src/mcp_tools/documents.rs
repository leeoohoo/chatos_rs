// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_mcp_contract::args::{
    ListRequirementTechnicalDocumentsArgs, RequirementTechnicalDocumentIdArgs,
    UpsertRequirementTechnicalDocumentArgs,
};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{UpdateRequirementDocumentRequest, UpsertRequirementDocumentRequest};
use crate::state::AppState;

use super::{
    decode_value, ensure_project_writable, ensure_requirement_mutable_for_mcp, normalized_optional,
    require_project_access, require_requirement_in_project, tool_text_result,
};

pub(super) async fn list_requirement_technical_documents(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListRequirementTechnicalDocumentsArgs = decode_value(arguments)?;
    require_requirement_in_project(state, &args.requirement_id, project_id, current_user).await?;
    let docs = state
        .store
        .list_requirement_documents(&args.requirement_id, args.doc_type)
        .await?;
    Ok(tool_text_result(json!(docs)))
}

pub(super) async fn get_requirement_technical_document(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: RequirementTechnicalDocumentIdArgs = decode_value(arguments)?;
    require_requirement_in_project(state, &args.requirement_id, project_id, current_user).await?;
    let doc = state
        .store
        .get_requirement_document_by_id(&args.requirement_id, &args.document_id)
        .await?
        .ok_or_else(|| format!("需求技术文档不存在: {}", args.document_id))?;
    Ok(tool_text_result(json!(doc)))
}

pub(super) async fn upsert_requirement_technical_document(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpsertRequirementTechnicalDocumentArgs = decode_value(arguments)?;
    let requirement =
        require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
            .await?;
    let project = require_project_access(state, &requirement.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    let doc = if let Some(document_id) = normalized_optional(args.document_id) {
        state
            .store
            .update_requirement_document(
                &args.requirement_id,
                &document_id,
                UpdateRequirementDocumentRequest {
                    doc_type: args.doc_type,
                    title: args.title,
                    format: args.format,
                    content: Some(args.content),
                },
            )
            .await?
    } else {
        state
            .store
            .create_requirement_document(
                &args.requirement_id,
                UpsertRequirementDocumentRequest {
                    doc_type: args.doc_type,
                    title: args.title,
                    format: args.format,
                    content: args.content,
                },
                current_user,
            )
            .await?
    };
    Ok(tool_text_result(json!(doc)))
}
