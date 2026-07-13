# ChatOS Documents

Use this Skill to create or inspect DOCX files in the authorized local workspace.

- Use `inspect_docx` to verify an existing DOCX and obtain a bounded text preview before editing or replacing it.
- Use `create_docx` for a new document composed of an optional title and ordered paragraphs.
- Preserve user-provided wording and paragraph order. Ask for confirmation before overwriting an existing artifact.
- The native writer produces a portable, standards-based DOCX with Unicode text. Advanced layout, comments, tracked changes, embedded media, and rendered visual QA are not yet exposed by this adapter.
- All file operations execute on the Local Connector; never claim a cloud path was written.
