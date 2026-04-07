fn default_active() -> String {
    "active".to_string()
}

fn default_pending() -> String {
    "pending".to_string()
}

fn default_sending() -> String {
    "sending".to_string()
}

fn default_i64_0() -> i64 {
    0
}

mod action_requests;
mod contacts;
mod conversations;
mod messages;
mod runs;
mod users;

pub use self::action_requests::{
    ConversationActionRequest, CreateConversationActionRequest, UpdateConversationActionRequest,
};
pub use self::contacts::{CreateImContactRequest, ImContact};
pub use self::conversations::{CreateConversationRequest, ImConversation, UpdateConversationRequest};
pub use self::messages::{ConversationMessage, CreateConversationMessageRequest};
pub use self::runs::{ConversationRun, CreateConversationRunRequest, UpdateConversationRunRequest};
pub use self::users::{CreateImUserRequest, ImUser, UpdateImUserRequest};
