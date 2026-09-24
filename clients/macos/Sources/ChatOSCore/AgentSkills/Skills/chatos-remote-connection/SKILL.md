---
name: chatos-remote-connection
description: Operate one program-bound remote connection through SSH for connection tests, bounded commands, directory listing, file reads, downloads, and uploads. Use only when the remote-connection tool family is exposed for the current run.
---

# Remote connection

The platform binds the remote connection and credentials. Never ask for, guess, or place a connection identifier, password, private key, verification code, or other credential in tool arguments or output.

Choose the narrowest operation:

- Use `list_directory` and `read_file` to inspect text without executing a command.
- Use `download_file` with `base64` for binary data; do not confuse remote-to-local download with local-to-remote upload.
- Use `upload_file` only when the requested remote path and replacement intent are clear. Set `overwrite=false` unless replacement is explicitly intended.
- Use `run_command` for a bounded, non-interactive command. Treat `timed_out`, a nonzero exit code, truncation, stdout, and stderr as separate evidence.
- Use `test_connection` to diagnose connection availability, not as a required preflight before every harmless read.

Do not broaden paths, reveal credentials, turn on `allow_dangerous` merely to bypass a rejection, or retry an ambiguous mutation after a timeout.

Read [references/commands-and-transfers.md](references/commands-and-transfers.md) for command risk, transfer direction, binary content, overwrite, timeout, and recovery scenarios.
