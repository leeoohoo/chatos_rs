#[derive(Debug, Clone)]
pub struct LocalNotepadImageWrite {
    pub image: LocalNotepadImage,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait LocalNotepadStore: Send + Sync {
    async fn initialize_notepad(&self, owner_user_id: &str) -> Result<u64, ClientStorageError>;

    async fn list_notepad_folders(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<String>, ClientStorageError>;

    async fn create_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        now_unix_ms: i64,
    ) -> Result<String, ClientStorageError>;

    async fn rename_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        from: &str,
        to: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn delete_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        recursive: bool,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn list_notepad_notes(
        &self,
        owner_user_id: &str,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<LocalNotepadNote>, ClientStorageError>;

    async fn create_notepad_note(
        &self,
        command: &IdempotentCommand,
        note_id: &str,
        owner_user_id: &str,
        folder: &str,
        title: &str,
        content: &str,
        tags: &[String],
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError>;

    async fn get_notepad_note(
        &self,
        owner_user_id: &str,
        note_id: &str,
    ) -> Result<Option<LocalNotepadNoteDetail>, ClientStorageError>;

    async fn update_notepad_note(
        &self,
        command: &IdempotentCommand,
        update: &UpdateNotepadNoteCommand,
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError>;

    async fn delete_notepad_note(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        note_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;

    async fn put_notepad_image(
        &self,
        command: &IdempotentCommand,
        write: &LocalNotepadImageWrite,
    ) -> Result<LocalNotepadImage, ClientStorageError>;
}
