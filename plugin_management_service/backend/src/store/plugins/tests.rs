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

fn release_record(plugin_id: &str, release_id: &str) -> PluginReleaseRecord {
    serde_json::from_value(json!({
        "id": release_id,
        "plugin_id": plugin_id,
        "version": "1.0.0",
        "manifest_schema_version": 3,
        "normalized_manifest": {
            "schemaVersion": 3, "name": "atomic-plugin", "version": "1.0.0",
            "description": "atomic sync contract", "author": {"name": "Contract"},
            "keywords": [], "skills": [], "mcpServers": [], "apps": [], "commands": [],
            "agents": [], "hooks": [], "ui": [],
            "interface": {
                "displayName": "Atomic Plugin", "shortDescription": "Atomic",
                "longDescription": "Atomic sync contract", "developerName": "Contract",
                "category": "productivity", "capabilities": [], "defaultPrompt": [],
                "screenshots": []
            },
            "dependencies": {"plugins": [], "executables": [], "supportedPlatforms": []},
            "permissions": []
        },
        "npm_package": {"name": "atomic-plugin", "version": "1.0.0", "integrity": "sha512-dGVzdA=="},
        "artifact_ref": "https://example.com/atomic-plugin.tgz",
        "artifact_sha256": "a".repeat(64),
        "signature": {
            "key_id": "key", "publisher_id": "contract", "marketplace_id": "marketplace",
            "algorithm": "ed25519", "signature_base64": "signature",
            "signed_at": "2026-09-18T00:00:00Z", "manifest_sha256": "b".repeat(64)
        },
        "supported_platforms": [],
        "components": [{
            "component_key": "skills/main", "kind": "skill_collection",
            "display_name": "Main Skill", "runtime_kind": "skill",
            "entrypoint": null, "required": false, "permissions": [], "metadata": {}
        }],
        "dependencies": {"plugins": [], "executables": [], "supportedPlatforms": []},
        "permissions": [], "release_channel": "stable",
        "published_at": "2026-09-18T00:00:00Z", "revoked_at": null
    }))
    .expect("valid release record")
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

#[tokio::test]
#[ignore = "requires PLUGIN_MANAGEMENT_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn plugin_catalog_sync_rolls_back_snapshot_when_materialization_fails() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let marketplace_id = format!("catalog-atomic-marketplace-{suffix}");
    let marketplace: PluginMarketplaceRecord = serde_json::from_value(json!({
        "id": marketplace_id,
        "name": format!("catalog-atomic-marketplace-name-{suffix}"),
        "visibility": "public",
        "source_kind": "admin_registry",
        "catalog_url": "https://example.com/catalog.json",
        "enabled": true,
        "trust_level": "trusted",
        "trusted_signing_keys": [],
        "last_catalog_revision": "revision-1",
        "last_synced_at": "2026-09-18T00:00:00Z"
    }))
    .expect("valid marketplace record");
    store
        .replace_plugin_marketplace(&marketplace)
        .await
        .expect("insert atomic marketplace");

    let mut plugin = catalog_record(
        format!("catalog-atomic-plugin-{suffix}"),
        &marketplace_id,
        false,
        "productivity",
        "atomic sync contract",
        vec!["atomic"],
    );
    plugin.updated_at = "not-a-timestamp".to_string();
    let document: PluginCatalogDocument = serde_json::from_value(json!({
        "schema_version": 1,
        "marketplace_id": marketplace_id,
        "revision": "revision-1",
        "issued_at": "2026-09-18T00:00:00Z",
        "signing_keys": [],
        "plugins": [plugin],
        "releases": [],
        "component_snapshots": [],
        "revoked_release_ids": [],
        "signature": {
            "key_id": "test", "marketplace_id": marketplace_id,
            "algorithm": "ed25519", "signature_base64": "test",
            "signed_at": "2026-09-18T00:00:00Z", "catalog_sha256": "test"
        }
    }))
    .expect("valid catalog document shape");
    let mut sync = PluginCatalogSyncRecord {
        marketplace_id: marketplace_id.clone(),
        revision: "revision-1".to_string(),
        issued_at: "2026-09-18T00:00:00Z".to_string(),
        catalog_sha256: "test".to_string(),
        catalog_authority_publisher_id: "test".to_string(),
        document,
        synced_at: "2026-09-18T00:00:00Z".to_string(),
    };
    let error = store
        .apply_plugin_catalog_sync(
            &sync,
            None,
            &marketplace,
            sync.document.plugins.as_slice(),
            &[],
            &[],
        )
        .await
        .expect_err("invalid materialized timestamp must abort sync");
    assert!(error.contains("timestamp") || error.contains("date/time"));
    assert!(store
        .get_plugin_catalog_sync(&marketplace_id)
        .await
        .expect("read rolled back snapshot")
        .is_none());
    assert!(store
        .get_plugin_catalog_entry(&sync.document.plugins[0].id)
        .await
        .expect("read rolled back plugin")
        .is_none());

    sync.document.plugins[0].updated_at = "2026-09-18T00:00:00Z".to_string();
    let release = release_record(
        sync.document.plugins[0].id.as_str(),
        format!("catalog-atomic-release-{suffix}").as_str(),
    );
    let snapshot = PluginComponentSnapshot {
        plugin_id: release.plugin_id.clone(),
        release_id: release.id.clone(),
        component: release.components[0].clone(),
        content_sha256: release.artifact_sha256.clone(),
        skill: None,
    };
    sync.document.releases = vec![release.clone()];
    sync.document.component_snapshots = vec![snapshot.clone()];
    assert!(store
        .apply_plugin_catalog_sync(
            &sync,
            None,
            &marketplace,
            sync.document.plugins.as_slice(),
            sync.document.releases.as_slice(),
            sync.document.component_snapshots.as_slice(),
        )
        .await
        .expect("commit valid atomic catalog sync"));
    assert_eq!(
        store
            .get_plugin_catalog_sync(&marketplace_id)
            .await
            .expect("read committed snapshot")
            .expect("committed snapshot")
            .revision,
        "revision-1"
    );
    assert!(store
        .get_plugin_catalog_entry(&sync.document.plugins[0].id)
        .await
        .expect("read committed plugin")
        .is_some());
    assert!(store
        .get_plugin_release(&release.id)
        .await
        .expect("read committed release")
        .is_some());
    assert_eq!(
        store
            .list_plugin_component_snapshots(&release.plugin_id, &release.id)
            .await
            .expect("read committed component snapshots"),
        vec![snapshot]
    );

    sqlx::query("DELETE FROM plugin_component_snapshots WHERE plugin_id=$1")
        .bind(&release.plugin_id)
        .execute(&store.pool)
        .await
        .expect("delete atomic snapshots");
    sqlx::query("DELETE FROM plugin_release_publication_states WHERE release_id=$1")
        .bind(&release.id)
        .execute(&store.pool)
        .await
        .expect("delete atomic release state");
    sqlx::query("DELETE FROM plugin_releases WHERE id=$1")
        .bind(&release.id)
        .execute(&store.pool)
        .await
        .expect("delete atomic release");
    sqlx::query("DELETE FROM plugin_catalog_entries WHERE marketplace_id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete atomic plugin");
    sqlx::query("DELETE FROM plugin_catalog_syncs WHERE marketplace_id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete atomic snapshot");
    sqlx::query("DELETE FROM plugin_marketplaces WHERE id=$1")
        .bind(&marketplace_id)
        .execute(&store.pool)
        .await
        .expect("delete atomic marketplace");
}
