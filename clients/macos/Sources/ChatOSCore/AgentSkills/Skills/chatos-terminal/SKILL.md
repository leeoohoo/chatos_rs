---
name: chatos-terminal
description: Route ChatOS project-terminal work between command execution, process observation, and process control. Use whenever a task needs the built-in terminal tools; do not use it for ordinary project file reads or staged file edits that have dedicated tools.
---

# ChatOS terminal router

Activate only the specialist needed for the next terminal action:

- Use `chatos-terminal-command-execution` to start a command with `execute_command`, including deciding between foreground and background execution.
- Use `chatos-terminal-process-observation` to inspect recent logs or observe an existing process without changing it.
- Use `chatos-terminal-process-control` to write to stdin, submit input, stop a process, or use the compatibility `process` entrypoint.

Terminal access does not expand the task or grant permission. Prefer dedicated project read/write tools when they express the operation directly. Keep the working directory inside the bound project, preserve user changes, and treat exit status plus relevant output as evidence rather than assuming that a command succeeded.

For a lifecycle spanning more than one mode, activate specialists just before their first use. A background command normally follows execution → observation; add control only if input or termination is actually needed.
