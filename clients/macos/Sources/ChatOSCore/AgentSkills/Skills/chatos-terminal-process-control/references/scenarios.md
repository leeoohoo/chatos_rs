# Process control scenarios

## Confirmed interactive prompt

Situation: a known background process explicitly asks for a non-secret answer.

Good: quote the observed prompt, send the narrow answer to that `terminal_id`, use `submit=true` only if Enter is expected, then poll for the result.

Bad: send `y` speculatively to every quiet process, or answer a permission/credential prompt on the user's behalf.

## Program accepts streaming input

Good: use `process_write` with `submit=false` when the protocol expects exact bytes without a newline, then observe the response.

Bad: always submit a newline; it can alter REPL, debugger, or protocol semantics.

## Graceful shutdown

Good: when the program exposes a documented quit command and the task authorizes stopping it, send that command, observe a bounded interval, and kill only if it fails to exit and force is justified.

Bad: kill immediately when a graceful, low-risk shutdown is known and practical.

## Forced termination

Good: verify the process identity and status, explain why continuation is unsafe or no longer needed, call `process_kill`, then confirm the terminal is no longer running.

Bad: kill a build simply because it produced no output for one interval, or kill unrelated user-started work.

## Ambiguous process identity

Good: stop and gather more evidence from `process_list`, logs, working directory, and command metadata. If ambiguity remains, request direction rather than mutating either process.

Bad: choose the most recent terminal and send input or kill it.

## Compatibility `process` entrypoint

Good: classify the requested action first. Use the observation Skill for `list`, `poll`, `log`, or `wait`; use this Skill for `write`, `submit`, `kill`, or `close`. Supply only fields relevant to the chosen action.

Bad: treat `process` as one undifferentiated capability or bypass the router because it exposes several actions in one schema.
