use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::secrets::{decrypt_optional_secret, encrypt_optional_secret};
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn decrypt_optional_secret_lossy(value: Option<String>) -> Option<String> {
    let fallback = value.clone();
    decrypt_optional_secret(value).unwrap_or(fallback)
}

fn decrypt_connection_for_read(mut connection: RemoteConnection) -> RemoteConnection {
    connection.password = decrypt_optional_secret_lossy(connection.password);
    connection.jump_password = decrypt_optional_secret_lossy(connection.jump_password);
    connection
}

fn encrypt_connection_for_storage(
    mut connection: RemoteConnection,
) -> Result<RemoteConnection, String> {
    connection.password = encrypt_optional_secret(connection.password)?;
    connection.jump_password = encrypt_optional_secret(connection.jump_password)?;
    Ok(connection)
}

fn normalize_doc(doc: &Document) -> Option<RemoteConnection> {
    let jump_enabled = doc
        .get_bool("jump_enabled")
        .ok()
        .or_else(|| doc.get_i64("jump_enabled").ok().map(|v| v != 0))
        .or_else(|| doc.get_i32("jump_enabled").ok().map(|v| v != 0))
        .unwrap_or(false);

    let connection = RemoteConnection {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        host: doc.get_str("host").ok()?.to_string(),
        port: doc.get_i64("port").unwrap_or(22),
        username: doc.get_str("username").ok()?.to_string(),
        auth_type: doc
            .get_str("auth_type")
            .unwrap_or("private_key")
            .to_string(),
        password: doc.get_str("password").ok().map(|s| s.to_string()),
        private_key_path: doc.get_str("private_key_path").ok().map(|s| s.to_string()),
        certificate_path: doc.get_str("certificate_path").ok().map(|s| s.to_string()),
        default_remote_path: doc
            .get_str("default_remote_path")
            .ok()
            .map(|s| s.to_string()),
        host_key_policy: doc
            .get_str("host_key_policy")
            .unwrap_or("strict")
            .to_string(),
        jump_enabled,
        jump_host: doc.get_str("jump_host").ok().map(|s| s.to_string()),
        jump_port: doc.get_i64("jump_port").ok(),
        jump_username: doc.get_str("jump_username").ok().map(|s| s.to_string()),
        jump_private_key_path: doc
            .get_str("jump_private_key_path")
            .ok()
            .map(|s| s.to_string()),
        jump_password: doc.get_str("jump_password").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
        last_active_at: doc.get_str("last_active_at").unwrap_or("").to_string(),
    };

    Some(decrypt_connection_for_read(connection))
}

pub async fn list_remote_connections(
    user_id: Option<String>,
) -> Result<Vec<RemoteConnection>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = filter_optional_user_id(user_id);
                let cursor = db
                    .collection::<Document>("remote_connections")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let items: Vec<RemoteConnection> =
                    collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                        .await?;
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query = build_select_all_with_optional_user_id(
                    "remote_connections",
                    user_id.is_some(),
                    true,
                );
                let mut q = sqlx::query_as::<_, RemoteConnectionRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(RemoteConnectionRow::to_remote_connection)
                    .map(decrypt_connection_for_read)
                    .collect())
            })
        },
    )
    .await
}

pub async fn get_remote_connection_by_id(id: &str) -> Result<Option<RemoteConnection>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("remote_connections")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, RemoteConnectionRow>(
                    "SELECT * FROM remote_connections WHERE id = ?",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row
                    .map(RemoteConnectionRow::to_remote_connection)
                    .map(decrypt_connection_for_read))
            })
        },
    )
    .await
}

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
                sqlx::query("INSERT INTO remote_connections (id, name, host, port, username, auth_type, password, private_key_path, certificate_path, default_remote_path, host_key_policy, jump_enabled, jump_host, jump_port, jump_username, jump_private_key_path, jump_password, user_id, created_at, updated_at, last_active_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
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
                    .bind(&conn_sqlite.jump_host)
                    .bind(conn_sqlite.jump_port)
                    .bind(&conn_sqlite.jump_username)
                    .bind(&conn_sqlite.jump_private_key_path)
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
                                "jump_host": data_mongo.jump_host,
                                "jump_port": data_mongo.jump_port,
                                "jump_username": data_mongo.jump_username,
                                "jump_private_key_path": data_mongo.jump_private_key_path,
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
                sqlx::query("UPDATE remote_connections SET name = ?, host = ?, port = ?, username = ?, auth_type = ?, password = ?, private_key_path = ?, certificate_path = ?, default_remote_path = ?, host_key_policy = ?, jump_enabled = ?, jump_host = ?, jump_port = ?, jump_username = ?, jump_private_key_path = ?, jump_password = ?, updated_at = ? WHERE id = ?")
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
                    .bind(&data_sqlite.jump_host)
                    .bind(data_sqlite.jump_port)
                    .bind(&data_sqlite.jump_username)
                    .bind(&data_sqlite.jump_private_key_path)
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
