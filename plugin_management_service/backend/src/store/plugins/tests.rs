// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::json;

use super::*;

async fn test_store() -> AppStore {
    let database_url = std::env::var("PLUGIN_MANAGEMENT_TEST_DATABASE_URL")
        .expect("PLUGIN_MANAGEMENT_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    AppStore::new(pool)
}

fn catalog_record(
    id: String,
    marketplace_id: &str,
    featured: bool,
    category: &str,
    description: &str,
    keywords: Vec<&str>,
) -> PluginCatalogRecord {
    serde_json::from_value(json!({
        "id": id,
        "plugin_key": format!("plugin-key-{id}"),
        "marketplace_id": marketplace_id,
        "name": format!("plugin-name-{id}"),
        "display_name": "Cursor Plugin",
        "description": description,
        "publisher": { "id": "contract", "name": "Contract", "verified": true },
        "interface": {
            "displayName": "Cursor Plugin",
            "shortDescription": description,
            "longDescription": description,
            "developerName": "Contract",
            "category": category
        },
        "keywords": keywords,
        "visibility": "public",
        "featured": featured,
        "enabled": true,
        "latest_release_id": "",
        "license": { "license_id": "MIT", "redistributable": true },
        "created_at": "2026-09-18T00:00:00Z",
        "updated_at": "2026-09-18T00:00:00Z"
    }))
    .expect("valid contract Plugin Catalog record")
}

#[test]
fn plugin_catalog_cursor_requires_all_fields() {
    let partial = PluginCatalogQuery {
        after_featured: Some(true),
        after_category: Some("productivity".to_string()),
        ..PluginCatalogQuery::default()
    };
    assert_eq!(
        partial.cursor().unwrap_err(),
        "all Plugin Catalog cursor fields must be provided together"
    );
}

#[tokio::test]
#[ignore = "requires PLUGIN_MANAGEMENT_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn plugin_catalog_cursor_and_search_preserve_contracts() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let marketplace_id = format!("catalog-contract-marketplace-{suffix}");
    sqlx::query("INSERT INTO plugin_marketplaces(id,name,owner_user_id,visibility,source_kind,catalog_url,enabled,trust_level,data) VALUES($1,$2,NULL,'public','contract',NULL,TRUE,'trusted','{}'::jsonb)")
        .bind(&marketplace_id)
        .bind(format!("catalog-contract-marketplace-name-{suffix}"))
        .execute(&store.pool)
        .await
        .expect("insert contract marketplace");

    let ids = (0..6)
        .map(|index| format!("catalog-contract-{suffix}-{index:02}"))
        .collect::<Vec<_>>();
    for (index, id) in ids.iter().enumerate() {
        let description = if index == 0 {
            "contains-description-needle"
        } else {
            "ordinary description"
        };
        let keywords = if index == 4 {
            vec!["contains-keyword-needle"]
        } else {
            vec!["ordinary"]
        };
        store
            .replace_plugin_catalog_entry(&catalog_record(
                id.clone(),
                &marketplace_id,
                index < 3,
                "productivity",
                description,
                keywords,
            ))
            .await
            .expect("insert contract catalog entry");
    }

    let first = store
        .list_plugin_catalog(
            &PluginCatalogQuery {
                marketplace_id: Some(marketplace_id.clone()),
                limit: Some(2),
                ..PluginCatalogQuery::default()
            },
            None,
        )
        .await
        .expect("first cursor page");
    assert_eq!(first.total, 6);
    assert_eq!(
        first.items.iter().map(|item| &item.id).collect::<Vec<_>>(),
        vec![&ids[0], &ids[1]]
    );

    let offset_page = store
        .list_plugin_catalog(
            &PluginCatalogQuery {
                marketplace_id: Some(marketplace_id.clone()),
                limit: Some(2),
                offset: Some(2),
                ..PluginCatalogQuery::default()
            },
            None,
        )
        .await
        .expect("legacy offset page");
    assert_eq!(offset_page.items[0].id, ids[2]);
    assert_eq!(offset_page.items[1].id, ids[3]);

    let inserted_before_cursor = format!("catalog-contract-{suffix}-new");
    store
        .replace_plugin_catalog_entry(&catalog_record(
            inserted_before_cursor.clone(),
            &marketplace_id,
            true,
            "000-before-cursor",
            "newer listing entry",
            vec!["ordinary"],
        ))
        .await
        .expect("insert entry before cursor");

    let mut seen = first
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let mut cursor = first.items.last().expect("first page cursor").clone();
    loop {
        let page = store
            .list_plugin_catalog(
                &PluginCatalogQuery {
                    marketplace_id: Some(marketplace_id.clone()),
                    after_featured: Some(cursor.featured),
                    after_category: Some(cursor.interface.category.clone()),
                    after_display_name: Some(cursor.display_name.clone()),
                    after_id: Some(cursor.id.clone()),
                    limit: Some(2),
                    ..PluginCatalogQuery::default()
                },
                None,
            )
            .await
            .expect("next cursor page");
        if page.items.is_empty() {
            break;
        }
        cursor = page.items.last().expect("page cursor").clone();
        seen.extend(page.items.into_iter().map(|item| item.id));
    }
    assert_eq!(seen.len(), ids.len());
    assert_eq!(seen.iter().collect::<HashSet<_>>().len(), ids.len());
    assert!(!seen.contains(&inserted_before_cursor));
    assert!(ids.iter().all(|id| seen.contains(id)));

    for (query, expected_id) in [("description-needle", &ids[0]), ("keyword-needle", &ids[4])] {
        let result = store
            .list_plugin_catalog(
                &PluginCatalogQuery {
                    marketplace_id: Some(marketplace_id.clone()),
                    q: Some(query.to_string()),
                    ..PluginCatalogQuery::default()
                },
                None,
            )
            .await
            .expect("search catalog");
        assert_eq!(result.total, 1);
        assert_eq!(&result.items[0].id, expected_id);
    }

    sqlx::query("DELETE FROM plugin_catalog_entries WHERE marketplace_id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete contract plugins");
    sqlx::query("DELETE FROM plugin_marketplaces WHERE id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete contract marketplace");
}
