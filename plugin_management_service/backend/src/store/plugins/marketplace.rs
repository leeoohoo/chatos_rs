// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

type OutboxRow = (String, i64, DateTime<Utc>, bool);

fn outbox_event(row: OutboxRow) -> PluginCatalogSyncOutboxEvent {
    PluginCatalogSyncOutboxEvent {
        marketplace_id: row.0,
        event_version: row.1,
        requested_at: row.2.to_rfc3339(),
        scheduled: row.3,
    }
}

async fn write_marketplace<'e, E>(
    executor: E,
    record: &PluginMarketplaceRecord,
) -> Result<(), String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("INSERT INTO plugin_marketplaces(id,name,owner_user_id,visibility,source_kind,catalog_url,enabled,trust_level,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(id) DO UPDATE SET name=EXCLUDED.name,owner_user_id=EXCLUDED.owner_user_id,visibility=EXCLUDED.visibility,source_kind=EXCLUDED.source_kind,catalog_url=EXCLUDED.catalog_url,enabled=EXCLUDED.enabled,trust_level=EXCLUDED.trust_level,data=EXCLUDED.data")
        .bind(&record.id).bind(&record.name).bind(&record.owner_user_id).bind(&record.visibility)
        .bind(&record.source_kind).bind(&record.catalog_url).bind(record.enabled).bind(&record.trust_level)
        .bind(json(record)?).execute(executor).await.map(|_| ()).map_err(db_error)
}

impl AppStore {
    pub async fn list_plugin_marketplaces(&self) -> Result<Vec<PluginMarketplaceRecord>, String> {
        decode_all(
            sqlx::query_scalar(
                "SELECT data FROM plugin_marketplaces ORDER BY enabled DESC,trust_level,name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?,
        )
    }

    pub async fn get_plugin_marketplace(
        &self,
        id: &str,
    ) -> Result<Option<PluginMarketplaceRecord>, String> {
        fetch_one(
            "SELECT data FROM plugin_marketplaces WHERE id=$1",
            id,
            &self.pool,
        )
        .await
    }

    pub async fn find_plugin_marketplace_by_name(
        &self,
        name: &str,
    ) -> Result<Option<PluginMarketplaceRecord>, String> {
        decode_optional(
            sqlx::query_scalar("SELECT data FROM plugin_marketplaces WHERE name=$1")
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_error)?,
        )
    }

    pub async fn replace_plugin_marketplace(
        &self,
        record: &PluginMarketplaceRecord,
    ) -> Result<(), String> {
        write_marketplace(&self.pool, record).await
    }

    pub async fn replace_plugin_marketplace_with_catalog_sync(
        &self,
        record: &PluginMarketplaceRecord,
        request_sync: bool,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        write_marketplace(&mut *tx, record).await?;
        self.update_catalog_sync_request(&mut tx, &record.id, request_sync)
            .await?;
        tx.commit().await.map_err(db_error)
    }

    pub async fn replace_plugin_marketplace_if_matches(
        &self,
        expected: &PluginMarketplaceRecord,
        record: &PluginMarketplaceRecord,
    ) -> Result<bool, String> {
        let result = sqlx::query("UPDATE plugin_marketplaces SET name=$1,owner_user_id=$2,visibility=$3,source_kind=$4,catalog_url=$5,enabled=$6,trust_level=$7,data=$8 WHERE id=$9 AND data=$10")
            .bind(&record.name).bind(&record.owner_user_id).bind(&record.visibility).bind(&record.source_kind)
            .bind(&record.catalog_url).bind(record.enabled).bind(&record.trust_level).bind(json(record)?)
            .bind(&expected.id).bind(json(expected)?).execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn replace_plugin_marketplace_if_matches_with_catalog_sync(
        &self,
        expected: &PluginMarketplaceRecord,
        record: &PluginMarketplaceRecord,
        request_sync: bool,
    ) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let result = sqlx::query("UPDATE plugin_marketplaces SET name=$1,owner_user_id=$2,visibility=$3,source_kind=$4,catalog_url=$5,enabled=$6,trust_level=$7,data=$8 WHERE id=$9 AND data=$10")
            .bind(&record.name).bind(&record.owner_user_id).bind(&record.visibility).bind(&record.source_kind)
            .bind(&record.catalog_url).bind(record.enabled).bind(&record.trust_level).bind(json(record)?)
            .bind(&expected.id).bind(json(expected)?).execute(&mut *tx).await.map_err(db_error)?;
        if result.rows_affected() != 1 {
            tx.rollback().await.map_err(db_error)?;
            return Ok(false);
        }
        self.update_catalog_sync_request(&mut tx, &record.id, request_sync)
            .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(true)
    }

    async fn update_catalog_sync_request(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        marketplace_id: &str,
        request_sync: bool,
    ) -> Result<(), String> {
        if request_sync {
            sqlx::query("INSERT INTO plugin_catalog_sync_outbox(marketplace_id,event_version,consumed_version,pending,scheduled,requested_at) VALUES($1,1,0,TRUE,FALSE,now()) ON CONFLICT(marketplace_id) DO UPDATE SET event_version=plugin_catalog_sync_outbox.event_version+1,pending=TRUE,scheduled=FALSE,requested_at=now(),published_version=NULL")
                .bind(marketplace_id).execute(&mut **tx).await.map_err(db_error)?;
        } else {
            sqlx::query(
                "UPDATE plugin_catalog_sync_outbox SET pending=FALSE WHERE marketplace_id=$1",
            )
            .bind(marketplace_id)
            .execute(&mut **tx)
            .await
            .map_err(db_error)?;
        }
        Ok(())
    }

    pub async fn pending_plugin_catalog_sync_event(
        &self,
        marketplace_id: &str,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        let row = sqlx::query_as::<_, OutboxRow>("SELECT marketplace_id,event_version,requested_at,scheduled FROM plugin_catalog_sync_outbox WHERE marketplace_id=$1 AND pending")
            .bind(marketplace_id).fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(row.map(outbox_event))
    }

    pub async fn list_pending_plugin_catalog_sync_events(
        &self,
        limit: i64,
    ) -> Result<Vec<PluginCatalogSyncOutboxEvent>, String> {
        let rows = sqlx::query_as::<_, OutboxRow>("SELECT marketplace_id,event_version,requested_at,scheduled FROM plugin_catalog_sync_outbox WHERE pending ORDER BY requested_at,marketplace_id LIMIT $1")
            .bind(limit.clamp(1,10_000)).fetch_all(&self.pool).await.map_err(db_error)?;
        Ok(rows.into_iter().map(outbox_event).collect())
    }

    pub async fn mark_plugin_catalog_sync_event_published(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
    ) -> Result<bool, String> {
        let result = sqlx::query("UPDATE plugin_catalog_sync_outbox SET pending=FALSE,published_version=$1 WHERE marketplace_id=$2 AND event_version=$1 AND pending")
            .bind(event.event_version).bind(&event.marketplace_id).execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn plugin_catalog_sync_event_consumed(
        &self,
        marketplace_id: &str,
        event_version: i64,
    ) -> Result<Option<bool>, String> {
        sqlx::query_scalar::<_, bool>("SELECT COALESCE(o.event_version>$2 OR o.consumed_version>=$2,FALSE) FROM plugin_marketplaces m LEFT JOIN plugin_catalog_sync_outbox o ON o.marketplace_id=m.id WHERE m.id=$1")
            .bind(marketplace_id).bind(event_version).fetch_optional(&self.pool).await.map_err(db_error)
    }

    pub async fn complete_plugin_catalog_sync_event(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
        schedule_next: bool,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        if !schedule_next {
            sqlx::query("UPDATE plugin_catalog_sync_outbox SET consumed_version=$1,pending=FALSE WHERE marketplace_id=$2 AND event_version=$1 AND consumed_version<$1")
                .bind(event.event_version).bind(&event.marketplace_id).execute(&self.pool).await.map_err(db_error)?;
            return Ok(None);
        }
        let row = sqlx::query_as::<_, OutboxRow>("UPDATE plugin_catalog_sync_outbox SET consumed_version=$1,event_version=event_version+1,pending=TRUE,scheduled=TRUE,requested_at=now(),published_version=NULL WHERE marketplace_id=$2 AND event_version=$1 AND consumed_version<$1 RETURNING marketplace_id,event_version,requested_at,scheduled")
            .bind(event.event_version).bind(&event.marketplace_id).fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(row.map(outbox_event))
    }

    pub async fn mark_plugin_catalog_sync_event_dead_lettered(
        &self,
        event: &PluginCatalogSyncOutboxEvent,
        error: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query("UPDATE plugin_catalog_sync_outbox SET consumed_version=$1,pending=FALSE,dead_letter_version=$1,dead_lettered_at=now(),last_error=$2 WHERE marketplace_id=$3 AND event_version=$1 AND consumed_version<$1")
            .bind(event.event_version).bind(error).bind(&event.marketplace_id).execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn replay_dead_lettered_plugin_catalog_sync(
        &self,
        marketplace_id: &str,
        dead_letter_version: i64,
    ) -> Result<Option<PluginCatalogSyncOutboxEvent>, String> {
        let row = sqlx::query_as::<_, OutboxRow>("UPDATE plugin_catalog_sync_outbox o SET event_version=o.event_version+1,pending=TRUE,scheduled=FALSE,requested_at=now(),published_version=NULL,dead_letter_version=NULL,dead_lettered_at=NULL,last_error=NULL FROM plugin_marketplaces m WHERE o.marketplace_id=m.id AND o.marketplace_id=$1 AND m.enabled AND m.trust_level=$2 AND m.source_kind=ANY($3) AND COALESCE(m.catalog_url,'')<>'' AND o.event_version=$4 AND o.dead_letter_version=$4 AND o.consumed_version>=$4 AND NOT o.pending RETURNING o.marketplace_id,o.event_version,o.requested_at,o.scheduled")
            .bind(marketplace_id).bind(PLUGIN_TRUST_TRUSTED).bind(vec![PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY,PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY]).bind(dead_letter_version)
            .fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(row.map(outbox_event))
    }

    pub async fn recover_plugin_catalog_sync_events(&self, limit: i64) -> Result<u64, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let marketplace_ids = sqlx::query_scalar::<_, String>("SELECT m.id FROM plugin_marketplaces m LEFT JOIN plugin_catalog_sync_outbox o ON o.marketplace_id=m.id WHERE m.enabled AND m.trust_level=$1 AND m.source_kind=ANY($2) AND COALESCE(m.catalog_url,'')<>'' AND (o.marketplace_id IS NULL OR (o.event_version<=o.consumed_version AND COALESCE(o.dead_letter_version,-1)<o.event_version)) ORDER BY m.id LIMIT $3 FOR UPDATE OF m SKIP LOCKED")
            .bind(PLUGIN_TRUST_TRUSTED).bind(vec![PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY,PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY]).bind(limit.clamp(1,10_000))
            .fetch_all(&mut *tx).await.map_err(db_error)?;
        let mut recovered = 0_u64;
        for marketplace_id in marketplace_ids {
            let result = sqlx::query("INSERT INTO plugin_catalog_sync_outbox(marketplace_id,event_version,consumed_version,pending,scheduled,requested_at) VALUES($1,1,0,TRUE,FALSE,now()) ON CONFLICT(marketplace_id) DO UPDATE SET event_version=plugin_catalog_sync_outbox.event_version+1,pending=TRUE,scheduled=FALSE,requested_at=now(),published_version=NULL WHERE plugin_catalog_sync_outbox.event_version<=plugin_catalog_sync_outbox.consumed_version AND COALESCE(plugin_catalog_sync_outbox.dead_letter_version,-1)<plugin_catalog_sync_outbox.event_version")
                .bind(marketplace_id).execute(&mut *tx).await.map_err(db_error)?;
            recovered += result.rows_affected();
        }
        tx.commit().await.map_err(db_error)?;
        Ok(recovered)
    }

    pub async fn acquire_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
        lock_until: DateTime<Utc>,
    ) -> Result<bool, String> {
        let acquired = sqlx::query_scalar::<_, String>("INSERT INTO plugin_catalog_sync_locks(marketplace_id,lock_owner,lock_until) VALUES($1,$2,$3) ON CONFLICT(marketplace_id) DO UPDATE SET lock_owner=EXCLUDED.lock_owner,lock_until=EXCLUDED.lock_until WHERE plugin_catalog_sync_locks.lock_until<=now() OR plugin_catalog_sync_locks.lock_owner=EXCLUDED.lock_owner RETURNING lock_owner")
            .bind(marketplace_id).bind(lock_owner).bind(lock_until).fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(acquired.is_some())
    }

    pub async fn release_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
    ) -> Result<(), String> {
        sqlx::query(
            "DELETE FROM plugin_catalog_sync_locks WHERE marketplace_id=$1 AND lock_owner=$2",
        )
        .bind(marketplace_id)
        .bind(lock_owner)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db_error)
    }

    pub async fn renew_plugin_catalog_sync_lease(
        &self,
        marketplace_id: &str,
        lock_owner: &str,
        lock_until: DateTime<Utc>,
    ) -> Result<bool, String> {
        let result = sqlx::query("UPDATE plugin_catalog_sync_locks SET lock_until=$1 WHERE marketplace_id=$2 AND lock_owner=$3")
            .bind(lock_until).bind(marketplace_id).bind(lock_owner).execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbox_row_maps_to_versioned_event() {
        let requested_at = DateTime::parse_from_rfc3339("2026-08-05T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = outbox_event(("marketplace-1".to_string(), 3, requested_at, true));
        assert_eq!(event.marketplace_id, "marketplace-1");
        assert_eq!(event.event_version, 3);
        assert!(event.scheduled);
    }
}
