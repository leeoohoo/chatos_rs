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

pub fn parse_database_node(node_id: &str) -> Option<String> {
    node_id
        .strip_prefix("db:")
        .map(std::string::ToString::to_string)
}

pub fn parse_prefixed_2(node_id: &str, prefix: &str) -> Option<(String, String)> {
    let mut parts = node_id.split(':');
    let node_prefix = parts.next()?;
    let first = parts.next()?;
    let second = parts.next()?;
    if node_prefix != prefix {
        return None;
    }
    Some((first.to_string(), second.to_string()))
}

pub fn parse_prefixed_3(node_id: &str, prefix: &str) -> Option<(String, String, String)> {
    let mut parts = node_id.split(':');
    let node_prefix = parts.next()?;
    let first = parts.next()?;
    let second = parts.next()?;
    let third = parts.next()?;
    if node_prefix != prefix {
        return None;
    }
    Some((first.to_string(), second.to_string(), third.to_string()))
}

pub fn parse_prefixed_4(
    node_id: &str,
    prefix: &str,
) -> Option<(String, String, String, String)> {
    let mut parts = node_id.split(':');
    let node_prefix = parts.next()?;
    let first = parts.next()?;
    let second = parts.next()?;
    let third = parts.next()?;
    let fourth = parts.next()?;
    if node_prefix != prefix {
        return None;
    }
    Some((
        first.to_string(),
        second.to_string(),
        third.to_string(),
        fourth.to_string(),
    ))
}
