use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    error::{AppError, AppResult},
};

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

pub fn parse_table_node(node_id: &str) -> Option<(String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let table = parts.next()?;
    if prefix != "table" {
        return None;
    }
    Some((database.to_string(), table.to_string()))
}

pub fn parse_detail_node(node_id: &str) -> Option<(MetadataNodeType, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let object_name = parts.next()?;

    let node_type = match prefix {
        "table" => MetadataNodeType::Table,
        "view" => MetadataNodeType::View,
        _ => return None,
    };

    Some((node_type, database.to_string(), object_name.to_string()))
}

pub fn parse_index_node(node_id: &str) -> Option<(String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let table = parts.next()?;
    let index_name = parts.next()?;
    if prefix != "index" {
        return None;
    }
    Some((
        database.to_string(),
        table.to_string(),
        index_name.to_string(),
    ))
}

pub fn parse_trigger_node(node_id: &str) -> Option<(String, String, String)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?;
    let table = parts.next()?;
    let trigger_name = parts.next()?;
    if prefix != "trigger" {
        return None;
    }
    Some((
        database.to_string(),
        table.to_string(),
        trigger_name.to_string(),
    ))
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

pub fn scoped_database(datasource: &DataSource) -> Option<&str> {
    datasource
        .network
        .database
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn ensure_database_in_scope(datasource: &DataSource, database: &str) -> AppResult<()> {
    if let Some(scoped) = scoped_database(datasource) {
        if !scoped.eq_ignore_ascii_case(database) {
            return Err(AppError::BadRequest(format!(
                "database {database} is out of scope for this datasource"
            )));
        }
    }
    Ok(())
}
