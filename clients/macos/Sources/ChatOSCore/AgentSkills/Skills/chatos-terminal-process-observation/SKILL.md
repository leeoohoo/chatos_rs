---
name: chatos-terminal-process-observation
description: Observe ChatOS terminal processes and logs without mutating them. Use to list processes, poll incremental output, page logs, inspect recent terminal history, or wait briefly for a known process.
---

# Terminal process observation

Choose the least expensive observation that answers the current question:

- `process_list` discovers known project processes; include exited entries only when history matters.
- `process_poll` is the default for incremental progress and status of a known `terminal_id`.
- `process_log` pages retained output when exact earlier context is needed.
- `get_recent_logs` summarizes recent project terminal activity when no process ID is available.
- `process_wait` is for a bounded wait on a known process, not an open-ended substitute for progress polling.

Track offsets or incremental cursors returned by the tool so the same output is not repeatedly loaded into model context. Use short, purposeful observation intervals and stop when the next decision does not depend on new output. Distinguish running, exited successfully, exited with failure, timed out, and killed states.

Silence is not completion. Conversely, repeated unchanged polls are not progress: investigate whether the process is legitimately quiet, blocked for input, hung, or already detached before continuing to wait.

Read [references/scenarios.md](references/scenarios.md) for long builds, quiet services, missing process IDs, truncated logs, and stalled-process examples.
