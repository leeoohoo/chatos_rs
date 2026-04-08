use mongodb::bson::Document;

pub fn mongo_set_doc_from_optional_strings<'a, I>(fields: I) -> Document
where
    I: IntoIterator<Item = (&'a str, Option<String>)>,
{
    let mut set_doc = Document::new();
    for (field, value) in fields {
        if let Some(v) = value {
            set_doc.insert(field, v);
        }
    }
    set_doc
}

pub fn sqlite_update_parts_from_optional_strings<'a, I>(fields: I) -> (Vec<String>, Vec<String>)
where
    I: IntoIterator<Item = (&'a str, Option<String>)>,
{
    let mut assignments = Vec::new();
    let mut binds = Vec::new();
    for (field, value) in fields {
        if let Some(v) = value {
            assignments.push(format!("{} = ?", field));
            binds.push(v);
        }
    }
    (assignments, binds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_mongo_set_doc_from_optional_values() {
        let doc = mongo_set_doc_from_optional_strings([
            ("name", Some("demo".to_string())),
            ("description", None),
            ("root_path", Some("/tmp".to_string())),
        ]);

        assert_eq!(doc.get_str("name").ok(), Some("demo"));
        assert_eq!(doc.get_str("root_path").ok(), Some("/tmp"));
        assert!(doc.get("description").is_none());
    }

    #[test]
    fn builds_sqlite_update_parts_from_optional_values() {
        let (assignments, binds) = sqlite_update_parts_from_optional_strings([
            ("name", Some("demo".to_string())),
            ("description", None),
            ("root_path", Some("/tmp".to_string())),
        ]);

        assert_eq!(assignments, vec!["name = ?", "root_path = ?"]);
        assert_eq!(binds, vec!["demo", "/tmp"]);
    }
}
