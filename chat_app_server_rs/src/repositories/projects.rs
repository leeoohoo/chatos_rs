use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::{collect_and_map, sort_by_str_key_desc};
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::core::update_fields::{
    mongo_set_doc_from_optional_strings, sqlite_update_parts_from_optional_strings,
};
use crate::models::project::{Project, ProjectRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<Project> {
    Some(Project {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        root_path: doc.get_str("root_path").ok()?.to_string(),
        description: doc.get_str("description").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_projects(user_id: Option<String>) -> Result<Vec<Project>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = filter_optional_user_id(user_id);
                let cursor = db
                    .collection::<Document>("projects")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut items: Vec<Project> = collect_and_map(cursor, normalize_doc).await?;
                sort_by_str_key_desc(&mut items, |item| item.created_at.as_str());
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("projects", user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, ProjectRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_project()).collect())
            })
        },
    )
    .await
}

pub async fn get_project_by_id(id: &str) -> Result<Option<Project>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("projects")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, ProjectRow>("SELECT * FROM projects WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_project()))
            })
        },
    )
    .await
}

pub async fn create_project(project: &Project) -> Result<String, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let proj_mongo = project.clone();
    let proj_sqlite = project.clone();

    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(proj_mongo.id.clone())),
                ("name", Bson::String(proj_mongo.name.clone())),
                ("root_path", Bson::String(proj_mongo.root_path.clone())),
                ("description", proj_mongo.description.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("user_id", proj_mongo.user_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("projects").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(proj_mongo.id.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO projects (id, name, root_path, description, user_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&proj_sqlite.id)
                    .bind(&proj_sqlite.name)
                    .bind(&proj_sqlite.root_path)
                    .bind(&proj_sqlite.description)
                    .bind(&proj_sqlite.user_id)
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(proj_sqlite.id.clone())
            })
        }
    ).await
}

pub async fn update_project(
    id: &str,
    name: Option<String>,
    root_path: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let name_mongo = name.clone();
    let root_mongo = root_path.clone();
    let desc_mongo = description.clone();
    let name_sqlite = name.clone();
    let root_sqlite = root_path.clone();
    let desc_sqlite = description.clone();
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = mongo_set_doc_from_optional_strings([
                    ("name", name_mongo),
                    ("root_path", root_mongo),
                    ("description", desc_mongo),
                ]);
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("projects")
                    .update_one(doc! { "id": id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let (mut fields, binds) = sqlite_update_parts_from_optional_strings([
                    ("name", name_sqlite),
                    ("root_path", root_sqlite),
                    ("description", desc_sqlite),
                ]);
                fields.push("updated_at = ?".to_string());
                let query_sql = format!("UPDATE projects SET {} WHERE id = ?", fields.join(", "));
                let mut query = sqlx::query(&query_sql);
                for b in binds {
                    query = query.bind(b);
                }
                query = query.bind(&now_sqlite).bind(&id);
                query.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn delete_project(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("projects")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM projects WHERE id = ?")
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
