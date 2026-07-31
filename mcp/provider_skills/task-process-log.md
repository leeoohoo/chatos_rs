# Task Process Log MCP

This is a run-scoped system MCP exposed only inside the current Task Runner execution. Use `task_run_process_record_process` to leave short visible breadcrumbs that help the user and later reviewers understand what happened during the run.

Good entries include:

- the approach selected before making changes;
- a concrete root cause or important observation;
- existing code or platform capability that was reused;
- a verification result;
- a blocker that still needs user input or external state;
- the next useful step.

Keep entries concise and user-safe. Do not write hidden chain-of-thought, secrets, credentials, raw command dumps, large file contents, or unrelated drafts. This MCP records the current task's visible process log only; it is not a replacement for making code changes, running checks, or returning the final answer.
