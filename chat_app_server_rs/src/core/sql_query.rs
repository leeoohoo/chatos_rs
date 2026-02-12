pub fn append_optional_user_id_filter(
    query: &mut String,
    include_user_id_filter: bool,
    has_where_clause: bool,
) {
    if !include_user_id_filter {
        return;
    }
    if has_where_clause {
        query.push_str(" AND user_id = ?");
    } else {
        query.push_str(" WHERE user_id = ?");
    }
}

pub fn build_select_all_with_optional_user_id(
    table: &str,
    include_user_id_filter: bool,
    order_by_created_at_desc: bool,
) -> String {
    let mut query = format!("SELECT * FROM {}", table);
    append_optional_user_id_filter(&mut query, include_user_id_filter, false);
    if order_by_created_at_desc {
        query.push_str(" ORDER BY created_at DESC");
    }
    query
}

pub fn append_limit_offset_clause(query: &mut String, limit: Option<i64>, offset: i64) {
    if limit.is_some() {
        query.push_str(" LIMIT ?");
        if offset > 0 {
            query.push_str(" OFFSET ?");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_limit_offset_clause, append_optional_user_id_filter,
        build_select_all_with_optional_user_id,
    };

    #[test]
    fn builds_query_with_optional_user_filter_and_order() {
        let query = build_select_all_with_optional_user_id("projects", true, true);
        assert_eq!(
            query,
            "SELECT * FROM projects WHERE user_id = ? ORDER BY created_at DESC"
        );
    }

    #[test]
    fn append_user_filter_uses_and_when_where_exists() {
        let mut query = "SELECT * FROM mcp_configs WHERE id IN (?, ?)".to_string();
        append_optional_user_id_filter(&mut query, true, true);
        assert_eq!(
            query,
            "SELECT * FROM mcp_configs WHERE id IN (?, ?) AND user_id = ?"
        );
    }

    #[test]
    fn append_limit_offset_supports_limit_without_offset() {
        let mut query = "SELECT * FROM sessions ORDER BY created_at DESC".to_string();
        append_limit_offset_clause(&mut query, Some(10), 0);
        assert_eq!(
            query,
            "SELECT * FROM sessions ORDER BY created_at DESC LIMIT ?"
        );
    }

    #[test]
    fn append_limit_offset_supports_limit_with_offset() {
        let mut query = "SELECT * FROM sessions ORDER BY created_at DESC".to_string();
        append_limit_offset_clause(&mut query, Some(10), 5);
        assert_eq!(
            query,
            "SELECT * FROM sessions ORDER BY created_at DESC LIMIT ? OFFSET ?"
        );
    }
}
