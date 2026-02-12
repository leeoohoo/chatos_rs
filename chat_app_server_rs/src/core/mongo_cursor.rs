use futures::TryStreamExt;
use mongodb::{bson::Document, Cursor};

pub async fn collect_and_map<T, F>(
    mut cursor: Cursor<Document>,
    mut normalize: F,
) -> Result<Vec<T>, String>
where
    F: FnMut(&Document) -> Option<T>,
{
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
        if let Some(item) = normalize(&doc) {
            out.push(item);
        }
    }
    Ok(out)
}

pub async fn collect_documents(cursor: Cursor<Document>) -> Result<Vec<Document>, String> {
    collect_and_map(cursor, |doc| Some(doc.clone())).await
}

pub async fn collect_string_field(
    mut cursor: Cursor<Document>,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
        if let Ok(value) = doc.get_str(field) {
            out.push(value.to_string());
        }
    }
    Ok(out)
}

pub fn sort_by_str_key_desc<T, F>(items: &mut [T], key: F)
where
    F: Fn(&T) -> &str,
{
    items.sort_by(|a, b| key(b).cmp(key(a)));
}

pub fn sort_by_str_key_asc<T, F>(items: &mut [T], key: F)
where
    F: Fn(&T) -> &str,
{
    items.sort_by(|a, b| key(a).cmp(key(b)));
}

pub fn apply_offset_limit<T>(mut items: Vec<T>, offset: i64, limit: Option<i64>) -> Vec<T> {
    if offset > 0 {
        items = items.into_iter().skip(offset as usize).collect();
    }
    if let Some(l) = limit {
        items = items.into_iter().take(l as usize).collect();
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Item {
        created_at: String,
    }

    #[test]
    fn sorts_descending_by_key() {
        let mut items = vec![
            Item {
                created_at: "2024-01-01".to_string(),
            },
            Item {
                created_at: "2024-01-03".to_string(),
            },
            Item {
                created_at: "2024-01-02".to_string(),
            },
        ];

        sort_by_str_key_desc(&mut items, |i| i.created_at.as_str());
        let ordered: Vec<String> = items.into_iter().map(|i| i.created_at).collect();
        assert_eq!(ordered, vec!["2024-01-03", "2024-01-02", "2024-01-01"]);
    }

    #[test]
    fn applies_offset_limit() {
        let items = vec![1, 2, 3, 4, 5];
        let out = apply_offset_limit(items, 2, Some(2));
        assert_eq!(out, vec![3, 4]);
    }

    #[test]
    fn applies_offset_only() {
        let items = vec![1, 2, 3];
        let out = apply_offset_limit(items, 1, None);
        assert_eq!(out, vec![2, 3]);
    }
}
