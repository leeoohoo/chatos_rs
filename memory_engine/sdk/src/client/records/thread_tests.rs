use serde_json::json;

use crate::models::ThreadRecordsPageResponse;

#[test]
fn thread_records_page_response_deserializes_items_and_total() {
    let response: ThreadRecordsPageResponse = serde_json::from_value(json!({
        "items": [
            {
                "id": "rec-1",
                "thread_id": "thread-1",
                "tenant_id": "tenant-1",
                "source_id": "source-1",
                "external_record_id": null,
                "role": "user",
                "record_type": "message",
                "content": "hello",
                "structured_payload": null,
                "metadata": {
                    "origin": "sdk-test"
                },
                "summary_status": "pending",
                "summary_id": null,
                "summarized_at": null,
                "created_at": "2026-05-20T00:00:00Z"
            }
        ],
        "total": 7
    }))
    .expect("page response should deserialize");

    assert_eq!(response.total, 7);
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].id, "rec-1");
    assert_eq!(response.items[0].thread_id, "thread-1");
    assert_eq!(
        response.items[0].metadata.as_ref().unwrap()["origin"],
        "sdk-test"
    );
}
