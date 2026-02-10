use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;

use crate::models::session::{Session, SessionRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<Session> {
    let id = doc.get_str("id").ok()?.to_string();
    let title = doc.get_str("title").ok()?.to_string();
    let description = doc.get_str("description").ok().map(|s| s.to_string());
    let metadata = doc
        .get_str("metadata")
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let user_id = doc.get_str("user_id").ok().map(|s| s.to_string());
    let project_id = doc.get_str("project_id").ok().map(|s| s.to_string());
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    let updated_at = doc.get_str("updated_at").ok().unwrap_or("").to_string();
    Some(Session {
        id,
        title,
        description,
        metadata,
        user_id,
        project_id,
        created_at,
        updated_at,
    })
}

pub async fn create_session(data: &Session) -> Result<String, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let metadata_str = data.metadata.as_ref().map(|m| m.to_string());
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("title", Bson::String(data_mongo.title.clone())),
                ("description", data_mongo.description.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("metadata", metadata_mongo.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("user_id", data_mongo.user_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("project_id", data_mongo.project_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("sessions").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(data_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO sessions (id, title, description, metadata, user_id, project_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.title)
                    .bind(&data_sqlite.description)
                    .bind(metadata_sqlite.as_deref())
                    .bind(&data_sqlite.user_id)
                    .bind(&data_sqlite.project_id)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(data_sqlite.id.clone())
            })
        }
    ).await
}

pub async fn get_session_by_id(id: &str) -> Result<Option<Session>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("sessions")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SessionRow>("SELECT * FROM sessions WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_session()))
            })
        },
    )
    .await
}

pub async fn get_all_sessions(limit: Option<i64>, offset: i64) -> Result<Vec<Session>, String> {
    with_db(
        |db| {
            Box::pin(async move {
                let mut cursor = db
                    .collection::<Document>("sessions")
                    .find(doc! {}, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut sessions: Vec<Session> = docs
                    .into_iter()
                    .filter_map(|d| normalize_from_doc(&d))
                    .collect();
                sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if offset > 0 {
                    sessions = sessions.into_iter().skip(offset as usize).collect();
                }
                if let Some(l) = limit {
                    sessions = sessions.into_iter().take(l as usize).collect();
                }
                Ok(sessions)
            })
        },
        |pool| {
            Box::pin(async move {
                let mut query = "SELECT * FROM sessions ORDER BY created_at DESC".to_string();
                if let Some(l) = limit {
                    query.push_str(" LIMIT ?");
                    if offset > 0 {
                        query.push_str(" OFFSET ?");
                        let rows = sqlx::query_as::<_, SessionRow>(&query)
                            .bind(l)
                            .bind(offset)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| e.to_string())?;
                        return Ok(rows.into_iter().map(|r| r.to_session()).collect());
                    }
                    let rows = sqlx::query_as::<_, SessionRow>(&query)
                        .bind(l)
                        .fetch_all(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok(rows.into_iter().map(|r| r.to_session()).collect());
                }
                let rows = sqlx::query_as::<_, SessionRow>(&query)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_session()).collect())
            })
        },
    )
    .await
}

pub async fn get_sessions_by_user_project(
    user_id: Option<String>,
    project_id: Option<String>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Session>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            let project_id = project_id.clone();
            Box::pin(async move {
                let mut filter = Document::new();
                if let Some(uid) = user_id {
                    filter.insert("user_id", uid);
                }
                if let Some(pid) = project_id {
                    filter.insert("project_id", pid);
                }
                let mut cursor = db
                    .collection::<Document>("sessions")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut sessions: Vec<Session> = docs
                    .into_iter()
                    .filter_map(|d| normalize_from_doc(&d))
                    .collect();
                sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if offset > 0 {
                    sessions = sessions.into_iter().skip(offset as usize).collect();
                }
                if let Some(l) = limit {
                    sessions = sessions.into_iter().take(l as usize).collect();
                }
                Ok(sessions)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            let project_id = project_id.clone();
            Box::pin(async move {
                let mut query = "SELECT * FROM sessions WHERE 1=1".to_string();
                let mut binds: Vec<String> = Vec::new();
                if let Some(uid) = user_id {
                    query.push_str(" AND user_id = ?");
                    binds.push(uid);
                }
                if let Some(pid) = project_id {
                    query.push_str(" AND project_id = ?");
                    binds.push(pid);
                }
                query.push_str(" ORDER BY created_at DESC");
                if let Some(l) = limit {
                    query.push_str(" LIMIT ?");
                    if offset > 0 {
                        query.push_str(" OFFSET ?");
                    }
                    let mut q = sqlx::query_as::<_, SessionRow>(&query);
                    for b in &binds {
                        q = q.bind(b);
                    }
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                    return Ok(rows.into_iter().map(|r| r.to_session()).collect());
                }
                let mut q = sqlx::query_as::<_, SessionRow>(&query);
                for b in &binds {
                    q = q.bind(b);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_session()).collect())
            })
        },
    )
    .await
}

pub async fn delete_session(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("sessions")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("messages")
                    .delete_many(doc! { "session_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM sessions WHERE id = ?")
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

pub async fn update_session(
    id: &str,
    title: Option<String>,
    description: Option<String>,
    metadata: Option<Value>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let metadata_str = metadata.as_ref().map(|m| m.to_string());
    let title_mongo = title.clone();
    let title_sqlite = title.clone();
    let description_mongo = description.clone();
    let description_sqlite = description.clone();
    let metadata_mongo = metadata_str.clone();
    let metadata_sqlite = metadata_str.clone();

    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                if let Some(t) = title_mongo.clone() {
                    set_doc.insert("title", t);
                }
                if let Some(d) = description_mongo.clone() {
                    set_doc.insert("description", d);
                }
                if let Some(m) = metadata_mongo.clone() {
                    set_doc.insert("metadata", m);
                }
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("sessions")
                    .update_one(doc! { "id": &id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_clause = Vec::new();
                if title_sqlite.is_some() {
                    set_clause.push("title = ?");
                }
                if description_sqlite.is_some() {
                    set_clause.push("description = ?");
                }
                if metadata_sqlite.is_some() {
                    set_clause.push("metadata = ?");
                }
                set_clause.push("updated_at = ?");
                let query = format!("UPDATE sessions SET {} WHERE id = ?", set_clause.join(", "));
                let mut q = sqlx::query(&query);
                if let Some(t) = title_sqlite {
                    q = q.bind(t);
                }
                if let Some(d) = description_sqlite {
                    q = q.bind(d);
                }
                if let Some(m) = metadata_sqlite {
                    q = q.bind(m);
                }
                q = q.bind(&now_sqlite);
                q = q.bind(&id);
                q.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
