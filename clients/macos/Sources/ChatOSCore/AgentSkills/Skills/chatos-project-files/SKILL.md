---
name: chatos-project-files
description: Route work on files inside the bound ChatOS project between read-only discovery and transactional edits. Use for project file inspection, search, presentation, creation, modification, or deletion; not for terminal execution or files outside the bound project.
---

# ChatOS project files router

Activate the specialist that matches the next operation:

- Use `chatos-project-read` to list directories, search, read whole files or line ranges, and explicitly open a file in the pet workbench.
- Use `chatos-project-write` for any creation, replacement, append, deletion, commit, or abandonment of a staged edit session.

Read before writing. Preserve the user's existing work and project conventions. File paths remain inside the client-bound project; a Skill never expands filesystem access. When a task needs both modes, activate read first, establish current content and revision evidence, then activate write before opening the edit transaction.

Terminal commands are not a substitute for the transactional write tools. Use the terminal Router only when the task genuinely requires command execution such as builds, tests, generators, or version-control inspection.
