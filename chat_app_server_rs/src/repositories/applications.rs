use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::models::application::{Application, ApplicationRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};
use mongodb::bson::{doc, Bson, Document};

fn normalize_doc(doc: &Document) -> Option<Application> {
    Some(Application {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        url: doc.get_str("url").ok()?.to_string(),
        description: doc.get_str("description").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_applications(user_id: Option<String>) -> Result<Vec<Application>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = filter_optional_user_id(user_id);
                let cursor = db
                    .collection::<Document>("applications")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let items: Vec<Application> =
                    collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                        .await?;
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("applications", user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, ApplicationRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_app()).collect())
            })
        },
    )
    .await
}

pub async fn get_application_by_id(id: &str) -> Result<Option<Application>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("applications")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row =
                    sqlx::query_as::<_, ApplicationRow>("SELECT * FROM applications WHERE id = ?")
                        .bind(&id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_app()))
            })
        },
    )
    .await
}

pub async fn create_application(app: &Application) -> Result<Application, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let app_mongo = app.clone();
    let app_sqlite = app.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(app_mongo.id.clone())),
                ("name", Bson::String(app_mongo.name.clone())),
                ("url", Bson::String(app_mongo.url.clone())),
                ("description", crate::core::values::optional_string_bson(app_mongo.description.clone())),
                ("user_id", crate::core::values::optional_string_bson(app_mongo.user_id.clone())),
                ("enabled", Bson::Boolean(app_mongo.enabled)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("applications").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(app_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO applications (id, name, url, description, user_id, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&app_sqlite.id)
                    .bind(&app_sqlite.name)
                    .bind(&app_sqlite.url)
                    .bind(&app_sqlite.description)
                    .bind(&app_sqlite.user_id)
                    .bind(crate::core::values::bool_to_sqlite_int(app_sqlite.enabled))
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(app_sqlite.clone())
            })
        }
    ).await
}

pub async fn update_application(id: &str, updates: &Application) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let updates_mongo = updates.clone();
    let updates_sqlite = updates.clone();
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                set_doc.insert("name", updates_mongo.name.clone());
                set_doc.insert("url", updates_mongo.url.clone());
                set_doc.insert("description", crate::core::values::optional_string_bson(updates_mongo.description.clone()));
                set_doc.insert("enabled", Bson::Boolean(updates_mongo.enabled));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("applications").update_one(doc! { "id": id }, doc! { "$set": set_doc }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE applications SET name = ?, url = ?, description = ?, enabled = ?, updated_at = ? WHERE id = ?")
                    .bind(&updates_sqlite.name)
                    .bind(&updates_sqlite.url)
                    .bind(&updates_sqlite.description)
                    .bind(crate::core::values::bool_to_sqlite_int(updates_sqlite.enabled))
                    .bind(&now_sqlite)
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn delete_application(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("applications")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("mcp_config_applications")
                    .delete_many(doc! { "application_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("system_context_applications")
                    .delete_many(doc! { "application_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("agent_applications")
                    .delete_many(doc! { "application_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM applications WHERE id = ?")
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
