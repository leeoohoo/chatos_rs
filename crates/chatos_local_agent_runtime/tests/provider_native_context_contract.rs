// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_runtime::{ProviderNativeContextError, ProviderNativeContextWindow};
use serde_json::json;

#[test]
fn request_input_is_built_without_mutating_durable_context() {
    let window = ProviderNativeContextWindow::new(
        1,
        vec![json!({"type": "message", "role": "user", "content": "first"})],
    )
    .unwrap();
    let input = window
        .request_input(&[json!({
            "type": "message",
            "role": "user",
            "content": "second"
        })])
        .unwrap();
    assert_eq!(input.len(), 2);
    assert_eq!(window.items().len(), 1);
}

#[test]
fn formal_response_appends_exact_input_and_output_items() {
    let first = json!({"type": "message", "role": "user", "content": "first"});
    let second = json!({"type": "message", "role": "user", "content": "second"});
    let output = json!({"type": "message", "id": "msg-1", "content": []});
    let mut window = ProviderNativeContextWindow::new(1, vec![first.clone()]).unwrap();
    let commit = window
        .commit_response(std::slice::from_ref(&second), std::slice::from_ref(&output))
        .unwrap();

    assert_eq!(commit.retained_items, vec![first, second, output]);
    assert_eq!(commit.dropped_item_count, 0);
    assert_eq!(commit.newest_compaction_id, None);
    assert_eq!(window.items(), commit.retained_items);
}

#[test]
fn newest_compaction_drops_every_item_before_it_and_remains_opaque() {
    let old_compaction = json!({
        "type": "compaction",
        "id": "compaction-old",
        "encrypted_content": "opaque-old"
    });
    let newest_compaction = json!({
        "type": "compaction",
        "id": "compaction-new",
        "encrypted_content": "opaque-new",
        "unknown_provider_field": {"must": "survive"}
    });
    let after = json!({"type": "message", "id": "msg-after", "content": []});
    let mut window = ProviderNativeContextWindow::new(
        3,
        vec![
            json!({"type": "message", "id": "old"}),
            old_compaction,
            json!({"type": "message", "id": "middle"}),
        ],
    )
    .unwrap();

    let commit = window
        .commit_response(
            &[json!({"type": "message", "role": "user", "content": "next"})],
            &[newest_compaction.clone(), after.clone()],
        )
        .unwrap();

    assert_eq!(commit.generation, 3);
    assert_eq!(commit.dropped_item_count, 4);
    assert_eq!(
        commit.newest_compaction_id.as_deref(),
        Some("compaction-new")
    );
    assert_eq!(commit.retained_items, vec![newest_compaction, after]);
}

#[test]
fn provider_switch_rebuilds_a_new_generation_without_old_opaque_state() {
    let mut window = ProviderNativeContextWindow::new(
        1,
        vec![json!({
            "type": "compaction",
            "id": "old-provider-state",
            "encrypted_content": "opaque"
        })],
    )
    .unwrap();
    let semantic = json!({"type": "message", "role": "user", "content": "rebuilt"});
    window
        .rebuild_generation(2, vec![semantic.clone()])
        .unwrap();

    assert_eq!(window.generation(), 2);
    assert_eq!(window.items(), &[semantic]);
    assert_eq!(
        window.rebuild_generation(2, Vec::new()),
        Err(ProviderNativeContextError::NonIncreasingGeneration { current: 2 })
    );
}

#[test]
fn invalid_generation_and_non_object_items_fail_closed() {
    assert_eq!(
        ProviderNativeContextWindow::empty(0),
        Err(ProviderNativeContextError::InvalidGeneration)
    );
    assert_eq!(
        ProviderNativeContextWindow::new(1, vec![json!("not-an-item")]),
        Err(ProviderNativeContextError::InvalidItem { index: 0 })
    );
}
