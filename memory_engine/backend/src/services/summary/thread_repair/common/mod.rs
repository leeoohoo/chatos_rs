mod prepare;

pub(crate) use prepare::load_repair_summary_preparation;

pub(crate) const THREAD_REPAIR_JOB_TYPE: &str = "thread_repair";
pub(crate) const THREAD_REPAIR_COMPAT_JOB_TYPE: &str = "summary_review_repair";
pub(crate) const THREAD_REPAIR_COMPAT_TRIGGER_TYPE: &str = "manual_review_repair";
pub(crate) const THREAD_REPAIR_SELECTION_POLICY: &str = "all_pending_unsummarized_records";
