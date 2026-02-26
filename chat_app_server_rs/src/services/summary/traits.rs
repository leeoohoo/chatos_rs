use std::future::Future;
use std::pin::Pin;

use super::types::{PersistSummaryOutcome, PersistSummaryPayload, SummaryLlmRequest};

pub type SummaryBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait SummaryLlmClient: Send + Sync {
    fn summarize<'a>(
        &'a self,
        request: SummaryLlmRequest,
    ) -> SummaryBoxFuture<'a, Result<String, String>>;
}

pub trait SummaryStore: Send + Sync {
    fn persist_summary<'a>(
        &'a self,
        payload: PersistSummaryPayload,
    ) -> SummaryBoxFuture<'a, Result<PersistSummaryOutcome, String>>;
}
