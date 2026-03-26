use crate::db::Db;
use crate::models::Message;

use super::now_rfc3339;

mod aggregate_ops;
mod read_ops;
mod write_ops;

pub use self::aggregate_ops::list_session_ids_with_pending_messages_by_user;
pub use self::read_ops::{
    get_latest_user_message_by_session, get_message_by_id, list_messages_by_session,
    list_pending_messages,
};
pub use self::write_ops::{
    batch_create_messages, create_message, delete_message_by_id, delete_messages_by_session,
    mark_messages_summarized, upsert_message_sync, SyncMessageInput,
};

pub(super) fn collection(db: &Db) -> mongodb::Collection<Message> {
    db.collection::<Message>("messages")
}
