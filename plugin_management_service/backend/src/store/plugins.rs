// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod marketplace;
mod user_state;

const RETIRED_BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";

impl AppStore {
    pub async fn remove_retired_bundled_plugin_marketplaces(&self) -> Result<u64, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let mut marketplace_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_marketplaces WHERE id=$1 OR trust_level='bundled'",
        )
        .bind(RETIRED_BUNDLED_MARKETPLACE_ID)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        if !marketplace_ids
            .iter()
            .any(|id| id == RETIRED_BUNDLED_MARKETPLACE_ID)
        {
            marketplace_ids.push(RETIRED_BUNDLED_MARKETPLACE_ID.to_string());
        }
        let plugin_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_catalog_entries WHERE marketplace_id=ANY($1)",
        )
        .bind(&marketplace_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        let release_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM plugin_releases WHERE plugin_id=ANY($1)",
        )
        .bind(&plugin_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "DELETE FROM plugin_agent_bindings WHERE resource_kind=ANY($1) AND resource_id=ANY($2)",
        )
        .bind(vec![RESOURCE_KIND_PLUGIN, RESOURCE_KIND_PLUGIN_COMPONENT])
        .bind(&plugin_ids)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        for query in [
            "DELETE FROM plugin_installations WHERE plugin_id=ANY($1)",
            "DELETE FROM plugin_user_preferences WHERE plugin_id=ANY($1)",
            "DELETE FROM plugin_component_snapshots WHERE plugin_id=ANY($1)",
            "DELETE FROM plugin_oauth_connections WHERE plugin_id=ANY($1)",
            "DELETE FROM plugin_audit_logs WHERE plugin_id=ANY($1)",
        ] {
            sqlx::query(query)
                .bind(&plugin_ids)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
        }
        sqlx::query("DELETE FROM plugin_release_publication_states WHERE release_id=ANY($1)")
            .bind(&release_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM plugin_releases WHERE plugin_id=ANY($1)")
            .bind(&plugin_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM plugin_catalog_entries WHERE marketplace_id=ANY($1)")
            .bind(&marketplace_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM plugin_publishers WHERE marketplace_id=ANY($1)")
            .bind(&marketplace_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("DELETE FROM plugin_catalog_syncs WHERE marketplace_id=ANY($1)")
            .bind(&marketplace_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let audit_ids = marketplace_ids
            .iter()
            .map(|id| format!("marketplace:{id}"))
            .collect::<Vec<_>>();
        sqlx::query("DELETE FROM plugin_audit_logs WHERE plugin_id=ANY($1)")
            .bind(audit_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let deleted = sqlx::query("DELETE FROM plugin_marketplaces WHERE id=ANY($1)")
            .bind(&marketplace_ids)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?
            .rows_affected();
        tx.commit().await.map_err(db_error)?;
        Ok(deleted)
    }

    pub async fn delete_plugin_bindings_for_agent(&self, agent_key: &str) -> Result<(), String> {
        sqlx::query(
            "DELETE FROM plugin_agent_bindings WHERE agent_key=$1 AND resource_kind=ANY($2)",
        )
        .bind(agent_key)
        .bind(vec![RESOURCE_KIND_PLUGIN, RESOURCE_KIND_PLUGIN_COMPONENT])
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn list_plugin_publishers(
        &self,
        query: &PluginPublisherQuery,
        owner_user_id: Option<&str>,
    ) -> Result<ListResponse<PluginPublisherRecord>, String> {
        let marketplace = normalized(query.marketplace_id.as_deref());
        let status = normalized(query.status.as_deref());
        let total = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM plugin_publishers WHERE ($1::text IS NULL OR owner_user_id=$1) AND ($2::text IS NULL OR marketplace_id=$2) AND ($3::text IS NULL OR status=$3)")
            .bind(owner_user_id).bind(&marketplace).bind(&status).fetch_one(&self.pool).await.map_err(db_error)?;
        let items = decode_all(sqlx::query_scalar("SELECT data FROM plugin_publishers WHERE ($1::text IS NULL OR owner_user_id=$1) AND ($2::text IS NULL OR marketplace_id=$2) AND ($3::text IS NULL OR status=$3) ORDER BY updated_at DESC,(data->>'created_at')::timestamptz DESC LIMIT $4 OFFSET $5")
            .bind(owner_user_id).bind(marketplace).bind(status).bind(query.limit.unwrap_or(100).clamp(1,500)).bind(i64::try_from(query.offset.unwrap_or(0)).unwrap_or(i64::MAX))
            .fetch_all(&self.pool).await.map_err(db_error)?)?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }

    pub async fn get_plugin_publisher(
        &self,
        id: &str,
    ) -> Result<Option<PluginPublisherRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_publishers WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }

    pub async fn find_plugin_publisher(
        &self,
        marketplace_id: &str,
        publisher_id: &str,
    ) -> Result<Option<PluginPublisherRecord>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM plugin_publishers WHERE marketplace_id=$1 AND publisher_id=$2",
            )
            .bind(marketplace_id)
            .bind(publisher_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn replace_plugin_publisher(
        &self,
        record: &PluginPublisherRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_publishers(id,marketplace_id,publisher_id,owner_user_id,status,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET marketplace_id=EXCLUDED.marketplace_id,publisher_id=EXCLUDED.publisher_id,owner_user_id=EXCLUDED.owner_user_id,status=EXCLUDED.status,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(&record.id).bind(&record.marketplace_id).bind(&record.publisher_id).bind(&record.owner_user_id).bind(&record.status).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn replace_plugin_publisher_if_matches(
        &self,
        expected: &PluginPublisherRecord,
        record: &PluginPublisherRecord,
    ) -> Result<bool, String> {
        let result = sqlx::query("UPDATE plugin_publishers SET marketplace_id=$1,publisher_id=$2,owner_user_id=$3,status=$4,updated_at=$5,data=$6 WHERE id=$7 AND data=$8")
            .bind(&record.marketplace_id).bind(&record.publisher_id).bind(&record.owner_user_id).bind(&record.status).bind(timestamp(&record.updated_at)?).bind(json(record)?).bind(&expected.id).bind(json(expected)?)
            .execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn get_plugin_catalog_sync(
        &self,
        marketplace_id: &str,
    ) -> Result<Option<PluginCatalogSyncRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_catalog_syncs WHERE marketplace_id=$1",
            marketplace_id,
            &self.pool,
        )
        .await
    }

    pub async fn commit_plugin_catalog_sync(
        &self,
        record: &PluginCatalogSyncRecord,
        expected_revision: Option<&str>,
    ) -> Result<bool, String> {
        let data = json(record)?;
        let synced_at = timestamp(&record.synced_at)?;
        if let Some(expected) = expected_revision {
            return sqlx::query("UPDATE plugin_catalog_syncs SET synced_at=$1,data=$2 WHERE marketplace_id=$3 AND data->>'revision'=$4")
                .bind(synced_at).bind(data).bind(&record.marketplace_id).bind(expected).execute(&self.pool).await.map(|r| r.rows_affected()==1).map_err(db_error);
        }
        match sqlx::query(
            "INSERT INTO plugin_catalog_syncs(marketplace_id,synced_at,data) VALUES($1,$2,$3)",
        )
        .bind(&record.marketplace_id)
        .bind(synced_at)
        .bind(data)
        .execute(&self.pool)
        .await
        {
            Ok(_) => Ok(true),
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => Ok(false),
            Err(error) => Err(db_error(error)),
        }
    }

    pub async fn list_plugin_catalog(
        &self,
        query: &PluginCatalogQuery,
        visible_owner_user_id: Option<&str>,
    ) -> Result<ListResponse<PluginCatalogRecord>, String> {
        let marketplace = normalized(query.marketplace_id.as_deref());
        let category = normalized(query.category.as_deref());
        let visibility = normalized(query.visibility.as_deref());
        let search = normalized(query.q.as_deref()).map(|q| format!("%{q}%"));
        let owner_view = visible_owner_user_id.is_some();
        let (after_featured, after_category, after_display_name, after_id) = match query.cursor()? {
            Some((featured, category, display_name, id)) => {
                (Some(featured), Some(category), Some(display_name), Some(id))
            }
            None => (None, None, None, None),
        };
        let predicate = "(NOT $1 OR (enabled AND (visibility=$2 OR (visibility=$3 AND owner_user_id=$4)))) AND ($1 OR $5::text IS NULL OR visibility=$5) AND ($1 OR $6::bool IS NULL OR enabled=$6) AND ($7::text IS NULL OR marketplace_id=$7) AND ($8::text IS NULL OR category=$8) AND ($9::bool IS NULL OR featured=$9) AND ($10::text IS NULL OR lower(id) LIKE lower($10) OR lower(name) LIKE lower($10) OR lower(display_name) LIKE lower($10) OR lower(data->>'description') LIKE lower($10) OR (lower(plugin_catalog_keywords_search_text(data->'keywords')) LIKE lower($10) AND EXISTS(SELECT 1 FROM jsonb_array_elements_text(COALESCE(data->'keywords','[]')) keyword WHERE keyword ILIKE $10)))";
        let total_sql = format!("SELECT count(*) FROM plugin_catalog_entries WHERE {predicate}");
        let total = sqlx::query_scalar::<_, i64>(&total_sql)
            .bind(owner_view)
            .bind(PLUGIN_VISIBILITY_PUBLIC)
            .bind(PLUGIN_VISIBILITY_PRIVATE)
            .bind(visible_owner_user_id)
            .bind(&visibility)
            .bind(query.enabled)
            .bind(&marketplace)
            .bind(&category)
            .bind(query.featured)
            .bind(&search)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        let items_sql = format!("SELECT data FROM plugin_catalog_entries WHERE {predicate} AND ($11::bool IS NULL OR ((NOT featured),category,display_name,id)>((NOT $11),$12::text,$13::text,$14::text)) ORDER BY (NOT featured),category,display_name,id LIMIT $15 OFFSET $16");
        let items = decode_all(
            sqlx::query_scalar(&items_sql)
                .bind(owner_view)
                .bind(PLUGIN_VISIBILITY_PUBLIC)
                .bind(PLUGIN_VISIBILITY_PRIVATE)
                .bind(visible_owner_user_id)
                .bind(visibility)
                .bind(query.enabled)
                .bind(marketplace)
                .bind(category)
                .bind(query.featured)
                .bind(search)
                .bind(after_featured)
                .bind(after_category)
                .bind(after_display_name)
                .bind(after_id)
                .bind(query.limit.unwrap_or(100).clamp(1, 500))
                .bind(i64::try_from(query.offset.unwrap_or(0)).unwrap_or(i64::MAX))
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?,
        )?;
        Ok(ListResponse {
            items,
            total: u64::try_from(total).unwrap_or(u64::MAX),
        })
    }

    pub async fn get_plugin_catalog_entry(
        &self,
        id: &str,
    ) -> Result<Option<PluginCatalogRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_catalog_entries WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }

    pub async fn find_plugin_catalog_entry(
        &self,
        marketplace_id: &str,
        name: &str,
    ) -> Result<Option<PluginCatalogRecord>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM plugin_catalog_entries WHERE marketplace_id=$1 AND name=$2",
            )
            .bind(marketplace_id)
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn replace_plugin_catalog_entry(
        &self,
        record: &PluginCatalogRecord,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO plugin_catalog_entries(id,plugin_key,marketplace_id,owner_user_id,name,display_name,category,visibility,enabled,featured,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(id) DO UPDATE SET plugin_key=EXCLUDED.plugin_key,marketplace_id=EXCLUDED.marketplace_id,owner_user_id=EXCLUDED.owner_user_id,name=EXCLUDED.name,display_name=EXCLUDED.display_name,category=EXCLUDED.category,visibility=EXCLUDED.visibility,enabled=EXCLUDED.enabled,featured=EXCLUDED.featured,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(&record.id).bind(&record.plugin_key).bind(&record.marketplace_id).bind(&record.owner_user_id).bind(&record.name).bind(&record.display_name).bind(&record.interface.category).bind(&record.visibility).bind(record.enabled).bind(record.featured).bind(timestamp(&record.updated_at)?).bind(json(record)?)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn list_plugin_releases(
        &self,
        plugin_id: &str,
        include_revoked: bool,
    ) -> Result<Vec<PluginReleaseRecord>, String> {
        decode_all(sqlx::query_scalar("SELECT r.data FROM plugin_releases r LEFT JOIN plugin_release_publication_states s ON s.release_id=r.id WHERE r.plugin_id=$1 AND ($2 OR r.revoked_at IS NULL) AND COALESCE(s.ready,TRUE) ORDER BY r.published_at DESC,r.version DESC").bind(plugin_id).bind(include_revoked).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn get_plugin_release(
        &self,
        id: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        decode_optional(sqlx::query_scalar("SELECT r.data FROM plugin_releases r LEFT JOIN plugin_release_publication_states s ON s.release_id=r.id WHERE r.id=$1 AND COALESCE(s.ready,TRUE)").bind(id).fetch_optional(&self.pool).await.map_err(db_error)?)
    }

    pub async fn list_plugin_releases_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<PluginReleaseRecord>, String> {
        let ids = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        decode_all(sqlx::query_scalar("SELECT r.data FROM plugin_releases r LEFT JOIN plugin_release_publication_states s ON s.release_id=r.id WHERE r.id=ANY($1) AND COALESCE(s.ready,TRUE)").bind(ids).fetch_all(&self.pool).await.map_err(db_error)?)
    }

    pub async fn get_plugin_release_any_state(
        &self,
        id: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_releases WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }

    pub async fn find_plugin_release_by_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<Option<PluginReleaseRecord>, String> {
        decode_optional(
            sqlx::query_scalar(
                "SELECT data FROM plugin_releases WHERE plugin_id=$1 AND version=$2",
            )
            .bind(plugin_id)
            .bind(version)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn insert_plugin_release_pending(
        &self,
        record: &PluginReleaseRecord,
    ) -> Result<(), String> {
        let state = PluginReleasePublicationState {
            release_id: record.id.clone(),
            ready: false,
            updated_at: now_rfc3339(),
        };
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query("INSERT INTO plugin_releases(id,plugin_id,version,release_channel,published_at,revoked_at,data) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&record.id).bind(&record.plugin_id).bind(&record.version).bind(&record.release_channel).bind(timestamp(&record.published_at)?).bind(optional_timestamp(record.revoked_at.as_deref())?).bind(json(record)?)
            .execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("INSERT INTO plugin_release_publication_states(release_id,ready,updated_at,data) VALUES($1,$2,$3,$4)")
            .bind(&state.release_id).bind(state.ready).bind(timestamp(&state.updated_at)?).bind(json(&state)?)
            .execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }

    pub async fn set_plugin_release_publication_ready(
        &self,
        release_id: &str,
        ready: bool,
    ) -> Result<(), String> {
        let state = PluginReleasePublicationState {
            release_id: release_id.to_string(),
            ready,
            updated_at: now_rfc3339(),
        };
        sqlx::query("INSERT INTO plugin_release_publication_states(release_id,ready,updated_at,data) VALUES($1,$2,$3,$4) ON CONFLICT(release_id) DO UPDATE SET ready=EXCLUDED.ready,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
            .bind(release_id).bind(ready).bind(timestamp(&state.updated_at)?).bind(json(&state)?).execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn replace_plugin_release(&self, record: &PluginReleaseRecord) -> Result<(), String> {
        sqlx::query("UPDATE plugin_releases SET plugin_id=$1,version=$2,release_channel=$3,published_at=$4,revoked_at=$5,data=$6 WHERE id=$7")
            .bind(&record.plugin_id).bind(&record.version).bind(&record.release_channel).bind(timestamp(&record.published_at)?).bind(optional_timestamp(record.revoked_at.as_deref())?).bind(json(record)?).bind(&record.id)
            .execute(&self.pool).await.map(|_| ()).map_err(db_error)
    }

    pub async fn replace_plugin_component_snapshots(
        &self,
        plugin_id: &str,
        release_id: &str,
        records: &[PluginComponentSnapshot],
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        sqlx::query("DELETE FROM plugin_component_snapshots WHERE plugin_id=$1 AND release_id=$2")
            .bind(plugin_id)
            .bind(release_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        for record in records {
            let value =
                serde_json::to_value(record.component.kind).map_err(|error| error.to_string())?;
            let kind = value
                .as_str()
                .ok_or_else(|| "Plugin component kind is not text".to_string())?;
            sqlx::query("INSERT INTO plugin_component_snapshots(plugin_id,release_id,component_key,component_kind,data) VALUES($1,$2,$3,$4,$5)")
                .bind(&record.plugin_id).bind(&record.release_id).bind(&record.component.component_key).bind(kind).bind(json(record)?).execute(&mut *tx).await.map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)
    }

    pub async fn list_plugin_component_snapshots(
        &self,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<PluginComponentSnapshot>, String> {
        decode_all(sqlx::query_scalar("SELECT data FROM plugin_component_snapshots WHERE plugin_id=$1 AND release_id=$2 ORDER BY component_key").bind(plugin_id).bind(release_id).fetch_all(&self.pool).await.map_err(db_error)?)
    }
}

#[cfg(test)]
mod tests;
