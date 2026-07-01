// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::Db;
use crate::models::EngineSource;

pub(crate) const RETIRED_SOURCE_IDS: &[&str] = &["memory_server"];

pub(crate) fn source_collection(db: &Db) -> mongodb::Collection<EngineSource> {
    db.collection::<EngineSource>("engine_sources")
}

pub(crate) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) fn normalize_optional_text_ref(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) fn tenant_bson(tenant_id: Option<&str>) -> Bson {
    match normalize_optional_text_ref(tenant_id) {
        Some(value) => Bson::String(value),
        None => Bson::Null,
    }
}

pub(crate) fn source_filter(tenant_id: Option<&str>, source_id: &str) -> Document {
    doc! {
        "tenant_id": tenant_bson(tenant_id),
        "source_id": source_id,
    }
}

pub(crate) fn generate_secret_key() -> String {
    format!("mse_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(crate) fn hash_secret(secret_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn build_secret_key_hint(secret_key: &str) -> String {
    let suffix_len = secret_key.len().min(6);
    format!("...{}", &secret_key[secret_key.len() - suffix_len..])
}
