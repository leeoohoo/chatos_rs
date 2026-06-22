use super::guidance;

pub struct ActiveConversationTurn {
    session_id: String,
    turn_id: String,
}

impl ActiveConversationTurn {
    pub fn start(session_id: &str, turn_id: &str) -> Self {
        guidance::register_active_turn(session_id, turn_id);
        Self {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
        }
    }
}

impl Drop for ActiveConversationTurn {
    fn drop(&mut self) {
        guidance::close_active_turn(&self.session_id, &self.turn_id);
    }
}
