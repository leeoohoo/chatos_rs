use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::models::terminal::{Terminal, TerminalRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<Terminal> {
    Some(Terminal {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        cwd: doc.get_str("cwd").ok()?.to_string(),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        project_id: doc.get_str("project_id").ok().map(|s| s.to_string()),
        status: doc.get_str("status").unwrap_or("running").to_string(),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
        last_active_at: doc.get_str("last_active_at").unwrap_or("").to_string(),
    })
}

pub async fn list_terminals(user_id: Option<String>) -> Result<Vec<Terminal>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = if let Some(uid) = user_id {
                    doc! { "user_id": uid }
                } else {
                    doc! {}
                };
                let mut cursor = db
                    .collection::<Document>("terminals")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut items: Vec<Terminal> =
                    docs.into_iter().filter_map(|d| normalize_doc(&d)).collect();
                items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let mut query = "SELECT * FROM terminals".to_string();
                if user_id.is_some() {
                    query.push_str(" WHERE user_id = ?");
                }
                query.push_str(" ORDER BY created_at DESC");
                let mut q = sqlx::query_as::<_, TerminalRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_terminal()).collect())
            })
        },
    )
    .await
}

pub async fn get_terminal_by_id(id: &str) -> Result<Option<Terminal>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("terminals")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, TerminalRow>("SELECT * FROM terminals WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_terminal()))
            })
        },
    )
    .await
}

pub async fn create_terminal(terminal: &Terminal) -> Result<String, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let term_mongo = terminal.clone();
    let term_sqlite = terminal.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(term_mongo.id.clone())),
                ("name", Bson::String(term_mongo.name.clone())),
                ("cwd", Bson::String(term_mongo.cwd.clone())),
                ("user_id", term_mongo.user_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("project_id", term_mongo.project_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("status", Bson::String(term_mongo.status.clone())),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
                ("last_active_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("terminals").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(term_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO terminals (id, name, cwd, user_id, project_id, status, created_at, updated_at, last_active_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&term_sqlite.id)
                    .bind(&term_sqlite.name)
                    .bind(&term_sqlite.cwd)
                    .bind(&term_sqlite.user_id)
                    .bind(&term_sqlite.project_id)
                    .bind(&term_sqlite.status)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(term_sqlite.id.clone())
            })
        }
    ).await
}

pub async fn update_terminal_status(
    id: &str,
    status: Option<String>,
    last_active_at: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let status_mongo = status.clone();
    let status_sqlite = status.clone();
    let last_mongo = last_active_at.clone().unwrap_or_else(|| now.clone());
    let last_sqlite = last_active_at.clone().unwrap_or_else(|| now.clone());
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                if let Some(v) = status_mongo {
                    set_doc.insert("status", v);
                }
                set_doc.insert("updated_at", now_mongo.clone());
                set_doc.insert("last_active_at", last_mongo.clone());
                db.collection::<Document>("terminals")
                    .update_one(doc! { "id": id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let mut fields = Vec::new();
                let mut binds: Vec<String> = Vec::new();
                if let Some(v) = status_sqlite {
                    fields.push("status = ?");
                    binds.push(v);
                }
                fields.push("updated_at = ?");
                fields.push("last_active_at = ?");
                let query_sql = format!("UPDATE terminals SET {} WHERE id = ?", fields.join(", "));
                let mut q = sqlx::query(&query_sql);
                for b in &binds {
                    q = q.bind(b);
                }
                q = q.bind(&now_sqlite).bind(&last_sqlite).bind(&id);
                q.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn touch_terminal(id: &str) -> Result<(), String> {
    update_terminal_status(id, None, Some(chrono::Utc::now().to_rfc3339())).await
}

pub async fn delete_terminal(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("terminals")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM terminals WHERE id = ?")
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
