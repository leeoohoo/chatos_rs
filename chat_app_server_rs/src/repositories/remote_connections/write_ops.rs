use mongodb::bson::{doc, Bson, Document};

use crate::models::remote_connection::RemoteConnection;
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

use super::encrypt_connection_for_storage;

pub async fn create_remote_connection(connection: &RemoteConnection) -> Result<String, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let conn_mongo = encrypt_connection_for_storage(connection.clone())?;
    let conn_sqlite = conn_mongo.clone();

    with_db(
        |db| {
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
                    crate::core::values::optional_string_bson(
                        conn_mongo.default_remote_path.clone(),
                    ),
                ),
                (
                    "host_key_policy",
                    Bson::String(conn_mongo.host_key_policy.clone()),
                ),
                ("jump_enabled", Bson::Boolean(conn_mongo.jump_enabled)),
                (
                    "jump_connection_id",
                    crate::core::values::optional_string_bson(
                        conn_mongo.jump_connection_id.clone(),
                    ),
                ),
                (
                    "jump_host",
                    crate::core::values::optional_string_bson(conn_mongo.jump_host.clone()),
                ),
                (
                    "jump_port",
                    conn_mongo
                        .jump_port
                        .map(Bson::Int64)
                        .unwrap_or(Bson::Null),
                ),
                (
                    "jump_username",
                    crate::core::values::optional_string_bson(conn_mongo.jump_username.clone()),
                ),
                (
                    "jump_private_key_path",
                    crate::core::values::optional_string_bson(
                        conn_mongo.jump_private_key_path.clone(),
                    ),
                ),
                (
                    "jump_certificate_path",
                    crate::core::values::optional_string_bson(
                        conn_mongo.jump_certificate_path.clone(),
                    ),
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
                db.collection::<Document>("remote_connections")
                    .insert_one(doc, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(conn_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO remote_connections (id, name, host, port, username, auth_type, password, private_key_path, certificate_path, default_remote_path, host_key_policy, jump_enabled, jump_connection_id, jump_host, jump_port, jump_username, jump_private_key_path, jump_certificate_path, jump_password, user_id, created_at, updated_at, last_active_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&conn_sqlite.id)
                    .bind(&conn_sqlite.name)
                    .bind(&conn_sqlite.host)
                    .bind(conn_sqlite.port)
                    .bind(&conn_sqlite.username)
                    .bind(&conn_sqlite.auth_type)
                    .bind(&conn_sqlite.password)
                    .bind(&conn_sqlite.private_key_path)
                    .bind(&conn_sqlite.certificate_path)
                    .bind(&conn_sqlite.default_remote_path)
                    .bind(&conn_sqlite.host_key_policy)
                    .bind(if conn_sqlite.jump_enabled { 1_i64 } else { 0_i64 })
                    .bind(&conn_sqlite.jump_connection_id)
                    .bind(&conn_sqlite.jump_host)
                    .bind(conn_sqlite.jump_port)
                    .bind(&conn_sqlite.jump_username)
                    .bind(&conn_sqlite.jump_private_key_path)
                    .bind(&conn_sqlite.jump_certificate_path)
                    .bind(&conn_sqlite.jump_password)
                    .bind(&conn_sqlite.user_id)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(conn_sqlite.id.clone())
            })
        },
    )
    .await
}

pub async fn update_remote_connection(id: &str, data: &RemoteConnection) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let id_mongo = id.to_string();
    let id_sqlite = id.to_string();
    let data_mongo = encrypt_connection_for_storage(data.clone())?;
    let data_sqlite = data_mongo.clone();

    with_db(
        |db| {
            Box::pin(async move {
                db.collection::<Document>("remote_connections")
                    .update_one(
                        doc! { "id": id_mongo },
                        doc! {
                            "$set": {
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
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("UPDATE remote_connections SET name = ?, host = ?, port = ?, username = ?, auth_type = ?, password = ?, private_key_path = ?, certificate_path = ?, default_remote_path = ?, host_key_policy = ?, jump_enabled = ?, jump_connection_id = ?, jump_host = ?, jump_port = ?, jump_username = ?, jump_private_key_path = ?, jump_certificate_path = ?, jump_password = ?, updated_at = ? WHERE id = ?")
                    .bind(&data_sqlite.name)
                    .bind(&data_sqlite.host)
                    .bind(data_sqlite.port)
                    .bind(&data_sqlite.username)
                    .bind(&data_sqlite.auth_type)
                    .bind(&data_sqlite.password)
                    .bind(&data_sqlite.private_key_path)
                    .bind(&data_sqlite.certificate_path)
                    .bind(&data_sqlite.default_remote_path)
                    .bind(&data_sqlite.host_key_policy)
                    .bind(if data_sqlite.jump_enabled { 1_i64 } else { 0_i64 })
                    .bind(&data_sqlite.jump_connection_id)
                    .bind(&data_sqlite.jump_host)
                    .bind(data_sqlite.jump_port)
                    .bind(&data_sqlite.jump_username)
                    .bind(&data_sqlite.jump_private_key_path)
                    .bind(&data_sqlite.jump_certificate_path)
                    .bind(&data_sqlite.jump_password)
                    .bind(&now_sqlite)
                    .bind(&id_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn touch_remote_connection(id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();

    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("remote_connections")
                    .update_one(
                        doc! { "id": id },
                        doc! { "$set": { "updated_at": now_mongo.clone(), "last_active_at": now_mongo } },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE remote_connections SET updated_at = ?, last_active_at = ? WHERE id = ?")
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn delete_remote_connection(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("remote_connections")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM remote_connections WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
