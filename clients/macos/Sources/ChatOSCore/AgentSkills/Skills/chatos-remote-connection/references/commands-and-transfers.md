# Remote commands and transfers

## Program-bound connection

The run already selects one remote connection. Tool schemas intentionally hide its internal ID and all credentials. If the selected connection is unavailable, report that state; do not ask the Human to disclose secrets or invent an identifier.

## Read and inspect

Good: list a narrow directory, then read the specific text file needed for the task with a bounded byte limit.

Bad: recursively enumerate unrelated paths, read likely secret stores, or execute `find /` when a direct path is available.

Use `download_file` with `encoding=base64` for binary or non-UTF-8 content. `read_file` is for UTF-8 text. A size-limit failure is a reason to narrow the request, not to repeat the same oversized read.

## Run a command

Prefer one non-interactive command with a realistic `timeout_seconds` and bounded output. A successful SSH transport does not imply command success: inspect `exit_code`, `timed_out`, `truncated`, stdout, and stderr.

Good: run a read-only status command in the known application directory and report its exit status.

Bad: start an interactive shell, launch an unobserved long-running process, concatenate unrelated mutations, or retry a timed-out mutation whose remote effect is unknown.

Set `allow_dangerous=true` only when the Human explicitly authorized the exact risky operation and its target. The flag acknowledges a narrow decision; it is not permission to widen scope.

## Upload and overwrite

`upload_file` sends model-provided content to the remote path. Confirm the destination and encoding. Use `overwrite=false` when creating a new artifact or when replacement was not explicitly requested. If the destination already exists, inspect or ask rather than silently replacing it.

For binary bytes, use valid Base64 and declare `encoding=base64`. Do not label plain text as Base64 or paste encoded binary into `read_file` workflows.

## Recovery

- Connection test fails: report the failure and stop remote mutations.
- Command times out: inspect returned evidence; do not assume rollback or completion.
- Output is truncated: narrow the command or request a smaller range rather than increasing output without bound.
- Upload result is uncertain: inspect the exact destination before retrying, especially when overwrite was enabled.
- Permission or path error: preserve the selected connection and requested boundary; do not switch hosts or escape to broader paths.
