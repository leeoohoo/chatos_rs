// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use super::{normalized_optional, DbStatus};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Draft,
    Reviewing,
    Approved,
    InProgress,
    Blocked,
    Done,
    Cancelled,
    Archived,
}

impl Default for RequirementStatus {
    fn default() -> Self {
        Self::Draft
    }
}

impl DbStatus for RequirementStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewing => "reviewing",
            Self::Approved => "approved",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Archived => "archived",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "reviewing" => Self::Reviewing,
            "approved" => Self::Approved,
            "in_progress" => Self::InProgress,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "archived" => Self::Archived,
            _ => Self::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementType {
    Requirement,
    Change,
    BugFix,
}

impl Default for RequirementType {
    fn default() -> Self {
        Self::Requirement
    }
}

impl DbStatus for RequirementType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Change => "change",
            Self::BugFix => "bug_fix",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "change" => Self::Change,
            "bug_fix" => Self::BugFix,
            _ => Self::Requirement,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementRecord {
    pub id: String,
    pub project_id: String,
    pub parent_requirement_id: Option<String>,
    #[serde(default)]
    pub requirement_type: RequirementType,
    pub title: String,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: i64,
    pub status: RequirementStatus,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub assignee_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequirementRequest {
    pub parent_requirement_id: Option<String>,
    pub requirement_type: Option<RequirementType>,
    pub title: String,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: Option<i64>,
    pub status: Option<RequirementStatus>,
    pub assignee_user_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequirementRequest {
    pub parent_requirement_id: Option<String>,
    pub requirement_type: Option<RequirementType>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: Option<i64>,
    pub status: Option<RequirementStatus>,
    pub assignee_user_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetRequirementDependenciesRequest {
    pub prerequisite_requirement_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDependencyRecord {
    pub requirement_id: String,
    pub prerequisite_requirement_id: String,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDocumentRecord {
    pub id: String,
    pub requirement_id: String,
    pub doc_type: String,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub title: String,
    pub format: String,
    pub content: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub const REQUIREMENT_TECHNICAL_OVERVIEW_DOC_TYPE: &str = "technical_overview";

pub fn normalize_requirement_document_type(value: Option<String>) -> Result<String, String> {
    let raw = normalized_optional(value)
        .unwrap_or_else(|| REQUIREMENT_TECHNICAL_OVERVIEW_DOC_TYPE.to_string());
    let doc_type = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if matches!(ch, '-' | ' ') { '_' } else { ch })
        .collect::<String>();
    if doc_type.len() > 64 {
        return Err("文档类型长度不能超过 64 个字符".to_string());
    }
    if !doc_type
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err("文档类型只能包含英文字母、数字、下划线、短横线或空格".to_string());
    }
    Ok(doc_type)
}

pub fn default_requirement_document_title(doc_type: &str) -> String {
    match doc_type {
        "technical_overview" => "实现技术总体文档",
        "implementation_plan" => "实现方案",
        "ui_svg_preview" => "前端 SVG 预览图",
        "architecture_diagram" => "架构图",
        "flowchart" => "流程图",
        "sequence_diagram" => "时序图",
        "api_design" => "接口设计",
        "data_model" => "数据模型",
        "risk_notes" => "风险说明",
        _ => "技术文档",
    }
    .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertRequirementDocumentRequest {
    #[serde(default)]
    pub doc_type: Option<String>,
    pub title: Option<String>,
    pub format: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequirementDocumentRequest {
    #[serde(default)]
    pub doc_type: Option<String>,
    pub title: Option<String>,
    pub format: Option<String>,
    pub content: Option<String>,
}
