use crate::domain::metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse};

pub fn paginate_nodes(
    items: Vec<MetadataNode>,
    page: u32,
    page_size: u32,
) -> MetadataNodesResponse {
    let safe_page = page.max(1);
    let safe_size = page_size.clamp(1, 500);
    let total = items.len() as u64;
    let start = ((safe_page - 1) * safe_size) as usize;

    let paged = if start >= items.len() {
        Vec::new()
    } else {
        let end = (start + safe_size as usize).min(items.len());
        items[start..end].to_vec()
    };

    MetadataNodesResponse {
        items: paged,
        page: safe_page,
        page_size: safe_size,
        total,
    }
}

pub fn parse_database_node(parent_id: &str) -> Option<String> {
    parent_id
        .strip_prefix("db:")
        .map(std::string::ToString::to_string)
}

pub fn parse_schema_node(parent_id: &str) -> Option<(String, String)> {
    let mut parts = parent_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    if prefix != "schema" {
        return None;
    }
    Some((database.to_string(), schema.to_string()))
}

pub fn parse_table_node(node_id: &str) -> Option<(String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let table = parts.next()?;
    if prefix != "table" {
        return None;
    }
    Some((database.to_string(), schema.to_string(), table.to_string()))
}

pub fn parse_detail_node(node_id: &str) -> Option<(MetadataNodeType, String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let object_name = parts.next()?;

    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        _ => return None,
    };

    Some((
        node_type,
        database.to_string(),
        schema.to_string(),
        object_name.to_string(),
    ))
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let table = parts.next()?;
    let index_name = parts.next()?;
    if prefix != "index" {
        return None;
    }
    Some((
        database.to_string(),
        schema.to_string(),
        table.to_string(),
        index_name.to_string(),
    ))
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let schema = parts.next()?;
    let table = parts.next()?;
    let trigger_name = parts.next()?;
    if prefix != "trigger" {
        return None;
    }
    Some((
        database.to_string(),
        schema.to_string(),
        table.to_string(),
        trigger_name.to_string(),
    ))
}

pub fn quote_sqlite_ident(value: &str) -> String {
    value.replace('"', "\"\"")
}

pub fn make_db_node(database: &str) -> MetadataNode {
    MetadataNode {
        id: format!("db:{database}"),
        parent_id: "root".to_string(),
        node_type: MetadataNodeType::Database,
        display_name: database.to_string(),
        path: database.to_string(),
        has_children: true,
    }
}
