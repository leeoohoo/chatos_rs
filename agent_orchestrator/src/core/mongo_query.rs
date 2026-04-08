use mongodb::bson::{doc, Document};

pub fn filter_optional_user_id(user_id: Option<String>) -> Document {
    if let Some(uid) = user_id {
        doc! { "user_id": uid }
    } else {
        doc! {}
    }
}

pub fn insert_optional_user_id(filter: &mut Document, user_id: Option<String>) {
    if let Some(uid) = user_id {
        filter.insert("user_id", uid);
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_optional_user_id, insert_optional_user_id};

    #[test]
    fn builds_empty_filter_when_user_is_missing() {
        let filter = filter_optional_user_id(None);
        assert!(filter.is_empty());
    }

    #[test]
    fn inserts_user_filter_when_present() {
        let mut filter = mongodb::bson::Document::new();
        insert_optional_user_id(&mut filter, Some("u-1".to_string()));
        assert_eq!(filter.get_str("user_id").ok(), Some("u-1"));
    }
}
