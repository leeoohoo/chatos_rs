---
name: chatos-project-read
description: Inspect and present files inside the bound ChatOS project without modifying them. Use for directory listing, fixed-text search, whole-file or range reads, and explicit requests to open a project file in the pet workbench.
---

# Project file reading

Choose the narrowest operation that establishes the needed fact:

- List a directory when structure or exact filenames are unknown.
- Search fixed text to locate symbols or references before reading files. Limit scope and results when possible.
- Read a line range when the relevant location is known; use a whole-file read only when complete context is genuinely needed.
- Treat compatibility aliases as the same capability: `read_file` maps to whole/range reads and `search_files` maps to fixed-text search.
- Use `open_file_in_pet` only when the Human explicitly asks to open, show, view, or edit a file in the desktop workbench. Reading a file for your own reasoning does not authorize opening UI.

Do not infer absence from truncated search or directory results. Respect returned limits, line numbering, hashes, and errors. Before a write, retain enough exact context or a content hash to detect concurrent changes.

Read [references/scenarios.md](references/scenarios.md) for large files, generated directories, ambiguous paths, search limits, UI opening, and read-before-write examples.
