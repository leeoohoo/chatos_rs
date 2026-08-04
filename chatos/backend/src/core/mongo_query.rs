// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};

pub fn filter_optional_user_id(user_id: Option<String>) -> Document {
    if let Some(uid) = user_id {
        doc! { "user_id": uid }
    } else {
        doc! {}
    }
}

#[cfg(test)]
mod tests {
    use super::filter_optional_user_id;

    #[test]
    fn builds_empty_filter_when_user_is_missing() {
        let filter = filter_optional_user_id(None);
        assert!(filter.is_empty());
    }
}
