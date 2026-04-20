use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataNodeType {
    ConnectionRoot,
    Database,
    Schema,
    Collection,
    Table,
    View,
    MaterializedView,
    Index,
    Sequence,
    Procedure,
    Function,
    Trigger,
    Synonym,
    Package,
    Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataNode {
    pub id: String,
    pub parent_id: String,
    pub node_type: MetadataNodeType,
    pub display_name: String,
    pub path: String,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataNodesResponse {
    pub items: Vec<MetadataNode>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct MetadataNodesQuery {
    pub datasource_id: String,
    pub parent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ObjectDetailQuery {
    pub datasource_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectConstraint {
    pub name: String,
    pub constraint_type: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectDetailResponse {
    pub node_id: String,
    pub node_type: MetadataNodeType,
    pub name: String,
    pub columns: Vec<ObjectColumn>,
    pub indexes: Vec<ObjectIndex>,
    pub constraints: Vec<ObjectConstraint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ddl: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStatsResponse {
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialized_view_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procedure_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synonym_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_count: Option<u64>,
    pub partial: bool,
}
