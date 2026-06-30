use super::support::test_store;
use sqlx::Row;

#[tokio::test]
async fn migrations_create_plan_snapshot_sort_indexes() {
    let store = test_store().await;
    let indexes = sqlx::query(
        "SELECT name FROM sqlite_master
         WHERE type = 'index'
           AND name IN (
             'idx_requirements_project_sort',
             'idx_requirements_project_status_sort',
             'idx_project_work_items_project_sort',
             'idx_project_work_items_project_status_sort',
             'idx_project_work_items_requirement_sort'
           )
         ORDER BY name",
    )
    .fetch_all(&store.pool)
    .await
    .expect("list indexes")
    .into_iter()
    .map(|row| row.get::<String, _>("name"))
    .collect::<Vec<_>>();

    assert_eq!(
        indexes,
        vec![
            "idx_project_work_items_project_sort",
            "idx_project_work_items_project_status_sort",
            "idx_project_work_items_requirement_sort",
            "idx_requirements_project_sort",
            "idx_requirements_project_status_sort",
        ]
    );
}
