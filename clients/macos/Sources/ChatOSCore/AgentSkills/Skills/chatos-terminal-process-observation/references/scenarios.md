# Process observation scenarios

## Background build

Good: poll the returned `terminal_id`, retain the next log offset, and switch to a final log page only if the failure context was truncated.

Bad: call `get_recent_logs` repeatedly and guess which terminal belongs to the build.

## Quiet but healthy server

Good: look for an explicit readiness line, listening port evidence, or a project-specific health check. If the process remains running without such evidence, report that readiness is unconfirmed.

Bad: declare success solely because no error appeared during one poll.

## Process appears stalled

Good: compare status and output across bounded polls. If output is unchanged, determine whether the program is waiting for input before deciding to write or kill; activate process control only when an action is warranted.

Bad: wait for the largest allowed duration with no intermediate decision point, or start duplicate copies of the command.

## Unknown terminal ID

Good: list project processes, correlate by command, working directory, start time, and status, and acknowledge ambiguity when multiple candidates match.

Bad: select the newest process based only on recency and then control it.

## Truncated or large logs

Good: use `process_log` offsets to read the narrow region around the first relevant error and its context. Record whether earlier or later content remains unread.

Bad: repeatedly request the entire log or interpret a truncated tail as the complete execution record.

## Process exited during observation

Good: treat the terminal exit status as authoritative, capture the final incremental output, and stop polling.

Bad: continue waiting because a prior snapshot said “running,” or infer a successful exit from a completion-looking log line while the exit status is nonzero.
