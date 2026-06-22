use super::render::summary_to_subject_memory_block;
use super::PendingSourceSummary;

#[test]
fn summary_memory_block_includes_summary_level() {
    let block = summary_to_subject_memory_block(&PendingSourceSummary {
        id: "sum-1".to_string(),
        tenant_id: "tenant".to_string(),
        source_id: "source".to_string(),
        thread_id: "thread".to_string(),
        summary_type: "thread_incremental".to_string(),
        level: 2,
        summary_text: "durable summary".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        metadata: None,
    });

    assert!(block.contains("[summary_id=sum-1]"));
    assert!(block.contains("[summary_type=thread_incremental]"));
    assert!(block.contains("[level=2]"));
    assert!(block.ends_with("durable summary"));
}
