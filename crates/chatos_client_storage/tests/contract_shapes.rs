// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_client_storage::{ListQuery, RecordScope};

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

#[test]
fn list_queries_are_bounded_for_both_backends() {
    assert_eq!(
        ListQuery {
            scope: scope(),
            cursor: None,
            limit: 0,
        }
        .validate(),
        Err("limit must be greater than zero")
    );
    assert_eq!(
        ListQuery {
            scope: scope(),
            cursor: None,
            limit: ListQuery::MAX_LIMIT + 1,
        }
        .validate(),
        Err("limit exceeds 500")
    );
    assert_eq!(
        ListQuery {
            scope: scope(),
            cursor: None,
            limit: ListQuery::MAX_LIMIT,
        }
        .validate(),
        Ok(())
    );
}
