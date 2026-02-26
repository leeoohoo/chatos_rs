use serde_json::{json, Value};

use super::traits::SummaryStore;
use super::types::{PersistSummaryOutcome, PersistSummaryPayload};

pub fn build_summary_metadata(payload: &PersistSummaryPayload) -> Value {
    json!({
        "algorithm": "bisect_v1",
        "trigger": payload.trigger.as_str(),
        "chunk_count": payload.stats.chunk_count,
        "max_depth": payload.stats.max_depth,
        "truncated": payload.truncated,
        "compression_ratio": payload.stats.compression_ratio,
        "input_tokens": payload.stats.input_tokens,
        "output_tokens": payload.stats.output_tokens
    })
}

pub async fn persist_summary<S: SummaryStore>(
    store: &S,
    payload: PersistSummaryPayload,
) -> Result<PersistSummaryOutcome, String> {
    store.persist_summary(payload).await
}
