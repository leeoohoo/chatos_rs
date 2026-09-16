// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, IndexOptions, ReturnDocument};
use mongodb::IndexModel;

use crate::models::{
    ClientSessionRecord, UserExternalIdentityRecord, WeChatBindTicketRecord,
    WECHAT_BIND_STATUS_CLAIMED, WECHAT_BIND_STATUS_CONFIRMED, WECHAT_BIND_STATUS_CONSUMED,
    WECHAT_BIND_STATUS_EXPIRED, WECHAT_BIND_STATUS_ISSUED,
};

use super::AppStore;

#[derive(Debug)]
pub enum BindExternalIdentityResult {
    Bound(UserExternalIdentityRecord),
    Conflict,
}

impl AppStore {
    pub(super) async fn initialize_wechat_auth_indexes(&self) -> Result<(), String> {
        self.create_unique_index(&self.user_external_identities, "id")
            .await?;
        self.create_compound_index(
            &self.user_external_identities,
            doc! { "provider": 1, "app_id": 1, "open_id_hash": 1 },
            "active_provider_subject_unique",
            true,
            Some(doc! { "revoked_at": null }),
        )
        .await?;
        self.create_compound_index(
            &self.user_external_identities,
            doc! { "user_id": 1, "provider": 1, "app_id": 1 },
            "active_user_provider_unique",
            true,
            Some(doc! { "revoked_at": null }),
        )
        .await?;

        self.create_unique_index(&self.wechat_bind_tickets, "id")
            .await?;
        self.create_unique_index(&self.wechat_bind_tickets, "ticket_hash")
            .await?;
        self.create_compound_index(
            &self.wechat_bind_tickets,
            doc! { "claim_id": 1 },
            "claim_id_unique_when_present",
            true,
            Some(doc! { "claim_id": { "$type": "string" } }),
        )
        .await?;
        self.create_index(&self.wechat_bind_tickets, "expires_at_unix")
            .await?;

        self.create_unique_index(&self.client_sessions, "id")
            .await?;
        self.create_unique_index(&self.client_sessions, "token_jti")
            .await?;
        self.create_compound_index(
            &self.client_sessions,
            doc! { "user_id": 1, "client_type": 1 },
            "user_client_type",
            false,
            None,
        )
        .await?;
        self.create_index(&self.client_sessions, "expires_at_unix")
            .await?;
        Ok(())
    }

    async fn create_compound_index<T>(
        &self,
        collection: &mongodb::Collection<T>,
        keys: mongodb::bson::Document,
        name: &str,
        unique: bool,
        partial_filter_expression: Option<mongodb::bson::Document>,
    ) -> Result<(), String>
    where
        T: Send + Sync,
    {
        let options = IndexOptions::builder()
            .name(name.to_string())
            .unique(unique)
            .partial_filter_expression(partial_filter_expression)
            .build();
        collection
            .create_index(
                IndexModel::builder().keys(keys).options(options).build(),
                None,
            )
            .await
            .map_err(|err| format!("create mongodb index {name} failed: {err}"))?;
        Ok(())
    }

    pub async fn find_active_external_identity_by_subject(
        &self,
        provider: &str,
        app_id: &str,
        open_id_hash: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        self.user_external_identities
            .find_one(
                doc! {
                    "provider": provider,
                    "app_id": app_id,
                    "open_id_hash": open_id_hash,
                    "revoked_at": null,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_active_external_identity_by_id(
        &self,
        id: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        self.user_external_identities
            .find_one(doc! { "id": id, "revoked_at": null }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_active_external_identity_for_user(
        &self,
        user_id: &str,
        provider: &str,
        app_id: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        self.user_external_identities
            .find_one(
                doc! {
                    "user_id": user_id,
                    "provider": provider,
                    "app_id": app_id,
                    "revoked_at": null,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn bind_external_identity(
        &self,
        record: &UserExternalIdentityRecord,
    ) -> Result<BindExternalIdentityResult, String> {
        if let Some(existing) = self
            .find_active_external_identity_by_subject(
                record.provider.as_str(),
                record.app_id.as_str(),
                record.open_id_hash.as_str(),
            )
            .await?
        {
            return Ok(if existing.user_id == record.user_id {
                BindExternalIdentityResult::Bound(existing)
            } else {
                BindExternalIdentityResult::Conflict
            });
        }
        if let Some(existing) = self
            .find_active_external_identity_for_user(
                record.user_id.as_str(),
                record.provider.as_str(),
                record.app_id.as_str(),
            )
            .await?
        {
            return Ok(if existing.open_id_hash == record.open_id_hash {
                BindExternalIdentityResult::Bound(existing)
            } else {
                BindExternalIdentityResult::Conflict
            });
        }

        let revoked = self
            .user_external_identities
            .find_one(
                doc! {
                    "user_id": &record.user_id,
                    "provider": &record.provider,
                    "app_id": &record.app_id,
                    "open_id_hash": &record.open_id_hash,
                    "revoked_at": { "$ne": null },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        let write_result = if let Some(revoked) = revoked {
            self.user_external_identities
                .find_one_and_update(
                    doc! { "id": &revoked.id, "revoked_at": { "$ne": null } },
                    doc! { "$set": {
                        "union_id_hash": &record.union_id_hash,
                        "updated_at": &record.updated_at,
                        "last_login_at": &record.last_login_at,
                        "revoked_at": null,
                    } },
                    FindOneAndUpdateOptions::builder()
                        .return_document(ReturnDocument::After)
                        .build(),
                )
                .await
                .map(|value| value.map(BindExternalIdentityResult::Bound))
        } else {
            self.user_external_identities
                .insert_one(record, None)
                .await
                .map(|_| Some(BindExternalIdentityResult::Bound(record.clone())))
        };

        match write_result {
            Ok(Some(result)) => Ok(result),
            Ok(None) => self.resolve_identity_bind_race(record).await,
            Err(err) if is_duplicate_key(&err) => self.resolve_identity_bind_race(record).await,
            Err(err) => Err(err.to_string()),
        }
    }

    async fn resolve_identity_bind_race(
        &self,
        record: &UserExternalIdentityRecord,
    ) -> Result<BindExternalIdentityResult, String> {
        let by_subject = self
            .find_active_external_identity_by_subject(
                record.provider.as_str(),
                record.app_id.as_str(),
                record.open_id_hash.as_str(),
            )
            .await?;
        let by_user = self
            .find_active_external_identity_for_user(
                record.user_id.as_str(),
                record.provider.as_str(),
                record.app_id.as_str(),
            )
            .await?;
        match (by_subject, by_user) {
            (Some(identity), _) if identity.user_id == record.user_id => {
                Ok(BindExternalIdentityResult::Bound(identity))
            }
            (_, Some(identity)) if identity.open_id_hash == record.open_id_hash => {
                Ok(BindExternalIdentityResult::Bound(identity))
            }
            _ => Ok(BindExternalIdentityResult::Conflict),
        }
    }

    pub async fn touch_external_identity_login(&self, id: &str, now: &str) -> Result<(), String> {
        self.user_external_identities
            .update_one(
                doc! { "id": id, "revoked_at": null },
                doc! { "$set": { "last_login_at": now, "updated_at": now } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn revoke_external_identity(
        &self,
        user_id: &str,
        provider: &str,
        app_id: &str,
        now: &str,
    ) -> Result<Option<UserExternalIdentityRecord>, String> {
        self.user_external_identities
            .find_one_and_update(
                doc! {
                    "user_id": user_id,
                    "provider": provider,
                    "app_id": app_id,
                    "revoked_at": null,
                },
                doc! { "$set": { "revoked_at": now, "updated_at": now } },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn insert_wechat_bind_ticket(
        &self,
        record: &WeChatBindTicketRecord,
    ) -> Result<(), String> {
        self.wechat_bind_tickets
            .insert_one(record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn expire_open_wechat_bind_tickets_for_user(
        &self,
        user_id: &str,
        app_id: &str,
        now: &str,
    ) -> Result<(), String> {
        self.wechat_bind_tickets
            .update_many(
                doc! {
                    "user_id": user_id,
                    "app_id": app_id,
                    "status": { "$in": [WECHAT_BIND_STATUS_ISSUED, WECHAT_BIND_STATUS_CLAIMED] },
                },
                doc! { "$set": {
                    "status": WECHAT_BIND_STATUS_EXPIRED,
                    "updated_at": now,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn find_wechat_bind_ticket_for_user(
        &self,
        ticket_id: &str,
        user_id: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        self.wechat_bind_tickets
            .find_one(doc! { "id": ticket_id, "user_id": user_id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn claim_wechat_bind_ticket(
        &self,
        ticket_hash: &str,
        app_id: &str,
        open_id_hash: &str,
        union_id_hash: Option<&str>,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        self.wechat_bind_tickets
            .find_one_and_update(
                doc! {
                    "ticket_hash": ticket_hash,
                    "app_id": app_id,
                    "status": WECHAT_BIND_STATUS_ISSUED,
                    "expires_at_unix": { "$gt": now_unix },
                },
                doc! { "$set": {
                    "status": WECHAT_BIND_STATUS_CLAIMED,
                    "claimed_open_id_hash": open_id_hash,
                    "claimed_union_id_hash": union_id_hash,
                    "claim_id": claim_id,
                    "claim_secret_hash": claim_secret_hash,
                    "claimed_at": now,
                    "updated_at": now,
                } },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn confirm_wechat_bind_ticket(
        &self,
        ticket_id: &str,
        user_id: &str,
        external_identity_id: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        self.wechat_bind_tickets
            .find_one_and_update(
                doc! {
                    "id": ticket_id,
                    "user_id": user_id,
                    "status": WECHAT_BIND_STATUS_CLAIMED,
                    "expires_at_unix": { "$gt": now_unix },
                },
                doc! { "$set": {
                    "status": WECHAT_BIND_STATUS_CONFIRMED,
                    "confirmed_external_identity_id": external_identity_id,
                    "confirmed_at": now,
                    "updated_at": now,
                } },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn consume_confirmed_wechat_claim(
        &self,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
        now: &str,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        self.wechat_bind_tickets
            .find_one_and_update(
                doc! {
                    "claim_id": claim_id,
                    "claim_secret_hash": claim_secret_hash,
                    "status": WECHAT_BIND_STATUS_CONFIRMED,
                    "expires_at_unix": { "$gt": now_unix },
                },
                doc! { "$set": {
                    "status": WECHAT_BIND_STATUS_CONSUMED,
                    "consumed_at": now,
                    "updated_at": now,
                } },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn find_wechat_claim(
        &self,
        claim_id: &str,
        claim_secret_hash: &str,
        now_unix: i64,
    ) -> Result<Option<WeChatBindTicketRecord>, String> {
        self.wechat_bind_tickets
            .find_one(
                doc! {
                    "claim_id": claim_id,
                    "claim_secret_hash": claim_secret_hash,
                    "expires_at_unix": { "$gt": now_unix },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn insert_client_session(&self, record: &ClientSessionRecord) -> Result<(), String> {
        self.client_sessions
            .insert_one(record, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn list_client_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<ClientSessionRecord>, String> {
        self.client_sessions
            .find(
                doc! { "user_id": user_id },
                FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .limit(100)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn revoke_client_session(
        &self,
        session_id: &str,
        user_id: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<Option<ClientSessionRecord>, String> {
        self.client_sessions
            .find_one_and_update(
                doc! { "id": session_id, "user_id": user_id, "revoked_at": null },
                doc! { "$set": {
                    "revoked_at": now,
                    "revoked_by": revoked_by,
                    "updated_at": now,
                } },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn revoke_client_session_by_jti(
        &self,
        token_jti: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<(), String> {
        self.client_sessions
            .update_one(
                doc! { "token_jti": token_jti, "revoked_at": null },
                doc! { "$set": {
                    "revoked_at": now,
                    "revoked_by": revoked_by,
                    "updated_at": now,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn touch_client_session(&self, token_jti: &str, now: &str) -> Result<(), String> {
        self.client_sessions
            .update_one(
                doc! {
                    "token_jti": token_jti,
                    "revoked_at": null,
                    "expires_at_unix": { "$gt": chrono::Utc::now().timestamp() },
                },
                doc! { "$set": {
                    "last_seen_at": now,
                    "updated_at": now,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn revoke_client_sessions_for_identity(
        &self,
        identity_id: &str,
        revoked_by: &str,
        now: &str,
    ) -> Result<Vec<ClientSessionRecord>, String> {
        let sessions: Vec<ClientSessionRecord> = self
            .client_sessions
            .find(
                doc! { "external_identity_id": identity_id, "revoked_at": null },
                None,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect()
            .await
            .map_err(|err| err.to_string())?;
        self.client_sessions
            .update_many(
                doc! { "external_identity_id": identity_id, "revoked_at": null },
                doc! { "$set": {
                    "revoked_at": now,
                    "revoked_by": revoked_by,
                    "updated_at": now,
                } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(sessions)
    }

    pub async fn is_client_session_invalid(
        &self,
        token_jti: &str,
        require_record: bool,
    ) -> Result<bool, String> {
        let Some(session) = self
            .client_sessions
            .find_one(doc! { "token_jti": token_jti }, None)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(require_record);
        };
        if session.revoked_at.is_some() || session.expires_at_unix <= chrono::Utc::now().timestamp()
        {
            return Ok(true);
        }
        let Some(identity_id) = session.external_identity_id.as_deref() else {
            return Ok(false);
        };
        let identity = self
            .user_external_identities
            .find_one(doc! { "id": identity_id, "revoked_at": null }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(identity.is_none())
    }
}

fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    error.to_string().contains("E11000") || error.to_string().contains("duplicate key")
}
