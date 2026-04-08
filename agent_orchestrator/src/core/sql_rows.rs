use sqlx::sqlite::SqliteRow;
use sqlx::Row;

pub fn collect_string_column(rows: Vec<SqliteRow>, column: &str) -> Vec<String> {
    rows.into_iter()
        .map(|row| row.try_get(column).unwrap_or_default())
        .collect()
}
