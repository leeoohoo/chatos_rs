use std::path::Path;

use async_trait::async_trait;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};

use crate::{
    domain::datasource::DataSource,
    error::{AppError, AppResult},
    repository::datasource_repo::DataSourceRepository,
};

pub struct SqliteDataSourceRepository {
    pool: SqlitePool,
}

impl SqliteDataSourceRepository {
    pub async fn new(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                AppError::BadRequest(format!(
                    "failed to create datasource store directory {}: {err}",
                    parent.display()
                ))
            })?;
        }

        let connect_options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .map_err(|err| {
                AppError::BadRequest(format!(
                    "failed to open sqlite datasource store {}: {err}",
                    path.display()
                ))
            })?;

        sqlx::query(
            "create table if not exists datasources (
                id text primary key,
                name text not null,
                db_type text not null,
                created_at text not null,
                updated_at text not null,
                payload text not null
            )",
        )
        .execute(&pool)
        .await
        .map_err(|err| {
            AppError::BadRequest(format!("failed to initialize datasource store: {err}"))
        })?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl DataSourceRepository for SqliteDataSourceRepository {
    async fn create(&self, datasource: DataSource) -> AppResult<()> {
        let payload = serde_json::to_string(&datasource).map_err(|err| {
            AppError::BadRequest(format!("failed to serialize datasource: {err}"))
        })?;

        let result = sqlx::query(
            "insert into datasources (id, name, db_type, created_at, updated_at, payload)
             values (?, ?, ?, ?, ?, ?)",
        )
        .bind(&datasource.id)
        .bind(&datasource.name)
        .bind(datasource.db_type.to_string())
        .bind(datasource.created_at.to_rfc3339())
        .bind(datasource.updated_at.to_rfc3339())
        .bind(payload)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(err) if is_unique_violation(&err) => Err(AppError::Conflict(format!(
                "datasource {} already exists",
                datasource.id
            ))),
            Err(err) => Err(AppError::BadRequest(format!(
                "failed to store datasource {}: {err}",
                datasource.id
            ))),
        }
    }

    async fn update(&self, datasource: DataSource) -> AppResult<()> {
        let payload = serde_json::to_string(&datasource).map_err(|err| {
            AppError::BadRequest(format!("failed to serialize datasource: {err}"))
        })?;

        let result = sqlx::query(
            "update datasources
             set name = ?, db_type = ?, updated_at = ?, payload = ?
             where id = ?",
        )
        .bind(&datasource.name)
        .bind(datasource.db_type.to_string())
        .bind(datasource.updated_at.to_rfc3339())
        .bind(payload)
        .bind(&datasource.id)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            AppError::BadRequest(format!(
                "failed to update datasource {}: {err}",
                datasource.id
            ))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "datasource {} not found",
                datasource.id
            )));
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> AppResult<()> {
        let result = sqlx::query("delete from datasources where id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| {
                AppError::BadRequest(format!("failed to delete datasource {id}: {err}"))
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("datasource {id} not found")));
        }

        Ok(())
    }

    async fn get(&self, id: &str) -> AppResult<Option<DataSource>> {
        let row = sqlx::query("select payload from datasources where id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| {
                AppError::BadRequest(format!("failed to load datasource {id}: {err}"))
            })?;

        match row {
            Some(row) => {
                let payload = row.try_get::<String, _>("payload").map_err(|err| {
                    AppError::BadRequest(format!("failed to read stored datasource payload: {err}"))
                })?;
                let datasource: DataSource = serde_json::from_str(&payload).map_err(|err| {
                    AppError::BadRequest(format!(
                        "failed to decode stored datasource {id} payload: {err}"
                    ))
                })?;
                Ok(Some(datasource))
            }
            None => Ok(None),
        }
    }

    async fn list(&self) -> AppResult<Vec<DataSource>> {
        let rows = sqlx::query("select payload from datasources order by datetime(created_at), id")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| AppError::BadRequest(format!("failed to list datasources: {err}")))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let payload = row.try_get::<String, _>("payload").map_err(|err| {
                AppError::BadRequest(format!("failed to read stored datasource payload: {err}"))
            })?;
            let datasource: DataSource = serde_json::from_str(&payload).map_err(|err| {
                AppError::BadRequest(format!("failed to decode stored datasource payload: {err}"))
            })?;
            items.push(datasource);
        }

        Ok(items)
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => db_err
            .message()
            .to_lowercase()
            .contains("unique constraint failed"),
        _ => false,
    }
}
