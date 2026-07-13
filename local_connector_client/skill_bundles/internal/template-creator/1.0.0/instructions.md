# ChatOS Template Creator

Use this Skill to turn a local DOCX, PDF, PPTX, XLSX, or CSV artifact into a reusable, integrity-checked ChatOS artifact template.

- Call `create_artifact_template` with a source artifact and a new workspace-relative template directory. The adapter copies the artifact and records its type, size, and SHA-256 hash in `template.json`.
- Call `inspect_artifact_template` before reuse to verify the stored artifact hash.
- Call `instantiate_artifact_template` to copy the verified immutable source artifact to a new workspace-relative target path.
- This version preserves the complete source artifact; it does not infer semantic placeholders or rewrite document content automatically.
- Overwriting a template directory or output artifact requires an explicit `overwrite=true` argument.
- All template packaging and instantiation happens on the Local Connector device.
