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

pub fn parse_schema_node(node_id: &str) -> Option<(String, String)> {
    let mut parts = node_id.split(':');
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

#[cfg(test)]
mod tests {
    use super::{parse_index_node, parse_trigger_node};

    #[test]
    fn parse_index_node_works() {
        let parsed = parse_index_node("index:orders_db:dbo:orders:IX_ORDERS_ID");
        assert!(parsed.is_some());
        let parsed = parsed.expect("index parser should return value");
        assert_eq!(parsed.0, "orders_db");
        assert_eq!(parsed.1, "dbo");
        assert_eq!(parsed.2, "orders");
        assert_eq!(parsed.3, "IX_ORDERS_ID");
    }

    #[test]
    fn parse_trigger_node_works() {
        let parsed = parse_trigger_node("trigger:orders_db:dbo:orders:TR_ORDERS_AUDIT");
        assert!(parsed.is_some());
        let parsed = parsed.expect("trigger parser should return value");
        assert_eq!(parsed.0, "orders_db");
        assert_eq!(parsed.1, "dbo");
        assert_eq!(parsed.2, "orders");
        assert_eq!(parsed.3, "TR_ORDERS_AUDIT");
    }
}
