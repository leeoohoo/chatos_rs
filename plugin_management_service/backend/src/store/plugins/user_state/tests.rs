// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use super::*;

async fn test_store() -> AppStore {
    let database_url = std::env::var("PLUGIN_MANAGEMENT_TEST_DATABASE_URL")
        .expect("PLUGIN_MANAGEMENT_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    AppStore::new(pool)
}

fn audit_record(id: String, plugin_id: &str, created_at: &str) -> PluginAuditLogRecord {
    PluginAuditLogRecord {
        id,
        event: "contract.cursor".to_string(),
        owner_user_id: "contract-owner".to_string(),
        device_id: Some("contract-device".to_string()),
        plugin_id: plugin_id.to_string(),
        release_id: None,
        component_key: None,
        outcome: "success".to_string(),
        details: BTreeMap::new(),
        created_at: created_at.to_string(),
    }
}

#[test]
fn plugin_audit_cursor_requires_a_complete_valid_pair() {
    let partial = PluginAuditQuery {
        before_created_at: Some("2026-09-18T00:00:00Z".to_string()),
        ..PluginAuditQuery::default()
    };
    assert_eq!(
        partial.cursor().unwrap_err(),
        "before_created_at and before_id must be provided together"
    );

    let malformed = PluginAuditQuery {
        before_created_at: Some("not-a-timestamp".to_string()),
        before_id: Some("audit-id".to_string()),
        ..PluginAuditQuery::default()
    };
    assert_eq!(
        malformed.cursor().unwrap_err(),
        "before_created_at must use RFC3339"
    );
}

#[tokio::test]
#[ignore = "requires PLUGIN_MANAGEMENT_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn plugin_audit_cursor_is_stable_and_offset_remains_compatible() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let marketplace_id = format!("contract-marketplace-{suffix}");
    let plugin_id = format!("contract-plugin-{suffix}");

    sqlx::query("INSERT INTO plugin_marketplaces(id,name,owner_user_id,visibility,source_kind,catalog_url,enabled,trust_level,data) VALUES($1,$2,NULL,'private','contract',NULL,TRUE,'trusted','{}'::jsonb)")
        .bind(&marketplace_id)
        .bind(format!("contract-marketplace-name-{suffix}"))
        .execute(&store.pool)
        .await
        .expect("insert contract marketplace");
    sqlx::query("INSERT INTO plugin_catalog_entries(id,plugin_key,marketplace_id,owner_user_id,name,display_name,category,visibility,enabled,featured,updated_at,data) VALUES($1,$2,$3,NULL,$4,$4,'contract','private',TRUE,FALSE,now(),'{}'::jsonb)")
        .bind(&plugin_id)
        .bind(format!("contract-plugin-key-{suffix}"))
        .bind(&marketplace_id)
        .bind(format!("contract-plugin-name-{suffix}"))
        .execute(&store.pool)
        .await
        .expect("insert contract plugin");

    let created_at = "2026-09-18T00:00:00Z";
    let ids = (0..6)
        .map(|index| format!("contract-audit-{suffix}-{index:02}"))
        .collect::<Vec<_>>();
    for id in &ids {
        store
            .insert_plugin_audit(&audit_record(id.clone(), &plugin_id, created_at))
            .await
            .expect("insert contract audit");
    }

    let first = store
        .list_plugin_audit(&PluginAuditQuery {
            plugin_id: Some(plugin_id.clone()),
            limit: Some(2),
            ..PluginAuditQuery::default()
        })
        .await
        .expect("first cursor page");
    assert_eq!(first.total, 6);
    assert_eq!(
        first.items.iter().map(|item| &item.id).collect::<Vec<_>>(),
        vec![&ids[5], &ids[4]]
    );

    let new_id = format!("contract-audit-{suffix}-new");
    store
        .insert_plugin_audit(&audit_record(
            new_id.clone(),
            &plugin_id,
            "2026-09-18T00:00:01Z",
        ))
        .await
        .expect("insert audit newer than cursor");

    let mut seen = first
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let mut cursor = first.items.last().expect("first page cursor").clone();
    loop {
        let page = store
            .list_plugin_audit(&PluginAuditQuery {
                plugin_id: Some(plugin_id.clone()),
                before_created_at: Some(cursor.created_at.clone()),
                before_id: Some(cursor.id.clone()),
                limit: Some(2),
                ..PluginAuditQuery::default()
            })
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
    assert!(!seen.contains(&new_id));
    assert!(ids.iter().all(|id| seen.contains(id)));

    let offset_page = store
        .list_plugin_audit(&PluginAuditQuery {
            plugin_id: Some(plugin_id.clone()),
            limit: Some(2),
            offset: Some(2),
            ..PluginAuditQuery::default()
        })
        .await
        .expect("legacy offset page");
    assert_eq!(offset_page.items[0].id, ids[4]);
    assert_eq!(offset_page.items[1].id, ids[3]);

    sqlx::query("DELETE FROM plugin_audit_logs WHERE plugin_id=$1")
        .bind(&plugin_id)
        .execute(&store.pool)
        .await
        .expect("delete contract audits");
    sqlx::query("DELETE FROM plugin_catalog_entries WHERE id=$1")
        .bind(&plugin_id)
        .execute(&store.pool)
        .await
        .expect("delete contract plugin");
    sqlx::query("DELETE FROM plugin_marketplaces WHERE id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete contract marketplace");
}
