---
name: chatos-terminal-process-control
description: Mutate an existing ChatOS terminal process by writing or submitting stdin, terminating it, or using the legacy process compatibility action. Use only after identifying the exact process and the intended state change.
---

# Terminal process control

Process control changes live execution state. Before acting, confirm the exact `terminal_id`, current status, owning command, and why the action is necessary.

- Use `process_write` for raw stdin data. Set `submit=true` only when the target expects the data followed by Enter.
- Use `process_kill` when the scoped task requires termination and graceful completion is unavailable or no longer appropriate.
- The compatibility `process` tool must follow the specialist for its selected action: list/poll/log/wait are observation; write/submit/kill/close are control. Prefer the dedicated tools for new flows.
- Never send credentials, secrets, approval answers, or guessed interactive responses through stdin.
- Never kill a process selected only by recency or a partial command match. Re-observe when identity or status may be stale.
- After a state-changing action, observe once to verify the resulting status. Do not equate a successful tool acknowledgement with the process having reached the intended state.

Read [references/scenarios.md](references/scenarios.md) before handling prompts, graceful shutdown, forced termination, ambiguous process identity, or the compatibility tool.
