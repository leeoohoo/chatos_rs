---
name: chatos-terminal-command-execution
description: Start commands in a bound ChatOS project safely and choose foreground versus background execution. Use for builds, tests, package commands, diagnostics, and other shell work; not for observing or controlling an already-started process.
---

# Terminal command execution

Before `execute_command`, identify the intended result, project-relative working directory, expected duration, side effects, and observable success signal.

- Use foreground execution when the command should finish within the requested timeout and its final exit status is needed immediately.
- Use `background=true` for servers, watchers, interactive programs, and work whose normal duration could exceed the foreground timeout. Preserve the returned `terminal_id` and continue with the observation Skill.
- Set a custom timeout only from evidence about expected runtime. A timeout is not proof that the underlying task failed; inspect the returned status and logs before deciding whether to retry.
- Quote or encode dynamic arguments as data. Never interpolate untrusted text into shell syntax without validating it.
- Do not hide failure with unconditional success operators. Report the actual command, scope, exit status, and the output that supports the conclusion.
- Do not rerun a side-effecting or expensive command merely because output was delayed. First determine whether the earlier process is still running or completed.

Read [references/scenarios.md](references/scenarios.md) when choosing a mode for a long-running, interactive, retry-sensitive, or ambiguous command.
