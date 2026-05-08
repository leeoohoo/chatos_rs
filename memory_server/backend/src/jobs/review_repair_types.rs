// REVIEW REPAIR COMPATIBILITY TYPES
// ---------------------------------
// 这些结构是 memory_server 兼容 chatos scope 时暴露给 review_repair 接口的返回类型。
// 真正的总结执行已经迁到 memory_engine，这里只保留兼容层需要的数据形状。

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopedSummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
    pub pending_message_count: i64,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopedReviewRepairStatus {
    pub running: bool,
    pub running_job_count: i64,
    pub pending_message_count: i64,
    pub scope_session_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_type: String,
}
