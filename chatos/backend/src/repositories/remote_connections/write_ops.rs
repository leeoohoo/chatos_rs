// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson};

use crate::models::remote_connection::RemoteConnection;
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_one_doc, mongo_insert_doc, mongo_update_set_doc, to_doc, with_db,
};

use super::encrypt_connection_for_storage;

pub async fn create_remote_connection(connection: &RemoteConnection) -> Result<String, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let conn_mongo = encrypt_connection_for_storage(connection.clone())?;

    with_db(|db| {
        let doc = to_doc(doc_from_pairs(vec![
            ("id", Bson::String(conn_mongo.id.clone())),
            ("name", Bson::String(conn_mongo.name.clone())),
            ("host", Bson::String(conn_mongo.host.clone())),
            ("port", Bson::Int64(conn_mongo.port)),
            ("username", Bson::String(conn_mongo.username.clone())),
            ("auth_type", Bson::String(conn_mongo.auth_type.clone())),
            (
                "password",
                crate::core::values::optional_string_bson(conn_mongo.password.clone()),
            ),
            (
                "private_key_path",
                crate::core::values::optional_string_bson(conn_mongo.private_key_path.clone()),
            ),
            (
                "certificate_path",
                crate::core::values::optional_string_bson(conn_mongo.certificate_path.clone()),
            ),
            (
                "default_remote_path",
                crate::core::values::optional_string_bson(conn_mongo.default_remote_path.clone()),
            ),
            (
                "host_key_policy",
                Bson::String(conn_mongo.host_key_policy.clone()),
            ),
            ("jump_enabled", Bson::Boolean(conn_mongo.jump_enabled)),
            (
                "jump_connection_id",
                crate::core::values::optional_string_bson(conn_mongo.jump_connection_id.clone()),
            ),
            (
                "jump_host",
                crate::core::values::optional_string_bson(conn_mongo.jump_host.clone()),
            ),
            (
                "jump_port",
                conn_mongo.jump_port.map(Bson::Int64).unwrap_or(Bson::Null),
            ),
            (
                "jump_username",
                crate::core::values::optional_string_bson(conn_mongo.jump_username.clone()),
            ),
            (
                "jump_private_key_path",
                crate::core::values::optional_string_bson(conn_mongo.jump_private_key_path.clone()),
            ),
            (
                "jump_certificate_path",
                crate::core::values::optional_string_bson(conn_mongo.jump_certificate_path.clone()),
            ),
            (
                "jump_password",
                crate::core::values::optional_string_bson(conn_mongo.jump_password.clone()),
            ),
            (
                "user_id",
                crate::core::values::optional_string_bson(conn_mongo.user_id.clone()),
            ),
            ("created_at", Bson::String(now_mongo.clone())),
            ("updated_at", Bson::String(now_mongo.clone())),
            ("last_active_at", Bson::String(now_mongo.clone())),
        ]));
        Box::pin(async move {
            mongo_insert_doc(db, "remote_connections", doc).await?;
            Ok(conn_mongo.id.clone())
        })
    })
    .await
}

pub async fn update_remote_connection(id: &str, data: &RemoteConnection) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let id_mongo = id.to_string();
    let data_mongo = encrypt_connection_for_storage(data.clone())?;

    with_db(|db| {
        Box::pin(async move {
            mongo_update_set_doc(
                db,
                "remote_connections",
                doc! { "id": id_mongo },
                doc! {
                    "name": data_mongo.name,
                    "host": data_mongo.host,
                    "port": data_mongo.port,
                    "username": data_mongo.username,
                    "auth_type": data_mongo.auth_type,
                    "password": data_mongo.password,
                    "private_key_path": data_mongo.private_key_path,
                    "certificate_path": data_mongo.certificate_path,
                    "default_remote_path": data_mongo.default_remote_path,
                    "host_key_policy": data_mongo.host_key_policy,
                    "jump_enabled": data_mongo.jump_enabled,
                    "jump_connection_id": data_mongo.jump_connection_id,
                    "jump_host": data_mongo.jump_host,
                    "jump_port": data_mongo.jump_port,
                    "jump_username": data_mongo.jump_username,
                    "jump_private_key_path": data_mongo.jump_private_key_path,
                    "jump_certificate_path": data_mongo.jump_certificate_path,
                    "jump_password": data_mongo.jump_password,
                    "updated_at": now_mongo,
                },
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub async fn touch_remote_connection(id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();

    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            mongo_update_set_doc(
                db,
                "remote_connections",
                doc! { "id": id },
                doc! { "updated_at": now_mongo.clone(), "last_active_at": now_mongo },
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub async fn delete_remote_connection(id: &str) -> Result<(), String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            mongo_delete_one_doc(db, "remote_connections", doc! { "id": &id }).await?;
            Ok(())
        })
    })
    .await
}
