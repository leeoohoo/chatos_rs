// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn folder_ancestors_preserve_nested_paths() {
    assert_eq!(
        folder_ancestors("projects/chatos/release"),
        vec!["projects", "projects/chatos", "projects/chatos/release"]
    );
}

#[test]
fn folder_rename_only_replaces_the_selected_prefix() {
    assert_eq!(replace_folder_prefix("a/b/c", "a/b", "x"), "x/c");
    assert_eq!(replace_folder_prefix("a/bb", "a/b", "x"), "a/bb");
}

#[tokio::test]
#[ignore = "requires CHATOS_TEST_DATABASE_URL and a migrated PostgreSQL database"]
async fn notes_survive_store_recreation_and_keep_revision_history() {
    let database_url =
        std::env::var("CHATOS_TEST_DATABASE_URL").expect("CHATOS_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    let user_id = format!("notepad-postgres-test:{}", Uuid::new_v4());
    let first = DatabaseNotepadStore::with_pool(&user_id, pool.clone());
    let created = first
        .create_note(CreateNoteParams {
            folder: "release/notes".to_string(),
            title: "Persistent".to_string(),
            content: "survives process replacement".to_string(),
            tags: vec!["database".to_string()],
        })
        .await
        .expect("create note");
    let note_id = created["note"]["id"].as_str().expect("note id");

    let recreated = DatabaseNotepadStore::with_pool(&user_id, pool.clone());
    let loaded = recreated.get_note(note_id).await.expect("load note");
    assert_eq!(loaded["content"], "survives process replacement");
    recreated
        .update_note(UpdateNoteParams {
            id: note_id.to_string(),
            title: Some("Persistent v2".to_string()),
            content: Some("still present after another replacement".to_string()),
            folder: None,
            tags: None,
        })
        .await
        .expect("update note");

    let recreated_again = DatabaseNotepadStore::with_pool(&user_id, pool.clone());
    let updated = recreated_again
        .get_note(note_id)
        .await
        .expect("load update");
    assert_eq!(
        updated["content"],
        "still present after another replacement"
    );
    recreated_again
        .delete_note(note_id)
        .await
        .expect("soft delete note");
    assert!(recreated_again.get_note(note_id).await.is_err());
    let revisions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notepad_note_revisions WHERE user_id=$1 AND note_id=$2",
    )
    .bind(&user_id)
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("revision count");
    assert_eq!(revisions, 3);
    let retained = sqlx::query_scalar::<_, bool>(
        "SELECT deleted_at IS NOT NULL FROM notepad_notes WHERE user_id=$1 AND id=$2",
    )
    .bind(&user_id)
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("soft-deleted row");
    assert!(retained);

    sqlx::query("DELETE FROM notepad_note_revisions WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete revisions");
    sqlx::query("DELETE FROM notepad_notes WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete notes");
    sqlx::query("DELETE FROM notepad_folders WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete folders");
}

#[tokio::test]
#[ignore = "requires CHATOS_TEST_DATABASE_URL and a migrated PostgreSQL database"]
async fn legacy_files_are_imported_once_without_overwriting_postgres() {
    let database_url =
        std::env::var("CHATOS_TEST_DATABASE_URL").expect("CHATOS_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    let suffix = Uuid::new_v4();
    let user_id = format!("notepad-import-test:{suffix}");
    let data_dir = std::env::temp_dir().join(format!("chatos-notepad-import-{suffix}"));
    let note_id = Uuid::new_v4().to_string();
    let note_dir = data_dir.join("notes").join("archive");
    tokio::fs::create_dir_all(&note_dir)
        .await
        .expect("create legacy note directory");
    tokio::fs::write(note_dir.join(format!("{note_id}.md")), "legacy content")
        .await
        .expect("write legacy note");
    let timestamp = now_iso();
    let index = super::super::types::NotesIndex {
        version: super::super::types::INDEX_VERSION,
        notes: vec![NoteIndexEntry {
            id: note_id.clone(),
            title: "Legacy".to_string(),
            folder: "archive".to_string(),
            tags: vec!["imported".to_string()],
            created_at: timestamp.clone(),
            updated_at: timestamp,
        }],
    };
    tokio::fs::write(
        data_dir.join("notes-index.json"),
        serde_json::to_vec(&index).expect("serialize legacy index"),
    )
    .await
    .expect("write legacy index");

    let legacy = NotepadStore::new(data_dir.clone());
    let store = DatabaseNotepadStore::with_pool(&user_id, pool.clone());
    store
        .ensure_legacy_imported(&legacy)
        .await
        .expect("import legacy files");
    let imported = store.get_note(&note_id).await.expect("read imported note");
    assert_eq!(imported["content"], "legacy content");

    tokio::fs::write(
        note_dir.join(format!("{note_id}.md")),
        "must not overwrite postgres",
    )
    .await
    .expect("change legacy source");
    let recreated = DatabaseNotepadStore::with_pool(&user_id, pool.clone());
    recreated
        .ensure_legacy_imported(&legacy)
        .await
        .expect("migration marker check");
    let unchanged = recreated
        .get_note(&note_id)
        .await
        .expect("read database note");
    assert_eq!(unchanged["content"], "legacy content");

    sqlx::query("DELETE FROM notepad_storage_migrations WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete migration marker");
    sqlx::query("DELETE FROM notepad_note_revisions WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete revisions");
    sqlx::query("DELETE FROM notepad_notes WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete notes");
    sqlx::query("DELETE FROM notepad_folders WHERE user_id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("delete folders");
    tokio::fs::remove_dir_all(data_dir)
        .await
        .expect("remove legacy fixture");
}
