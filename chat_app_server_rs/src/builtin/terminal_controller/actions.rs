pub(super) mod actions_execute;
pub(super) mod actions_process;
pub(super) mod actions_query;

const PROCESS_SNAPSHOT_TAIL_LINES: usize = 80;
const PROCESS_POLL_OFFSET_LIMIT_MAX: i64 = 500;
const PROCESS_WRITE_MAX_CHARS: usize = 32_768;
