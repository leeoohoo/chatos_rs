# ChatOS Template Creator

Use this Skill to turn a local DOCX, PPTX, XLSX, PDF, or CSV artifact into a reusable, integrity-checked ChatOS artifact template.

- Call `create_artifact_template` with a source artifact and a new workspace-relative template directory. The adapter copies the complete source artifact and records its type, size, SHA-256 hash, metadata, and optional semantic placeholder declarations in `template.json` schema v2.
- DOCX, PPTX, and XLSX templates may declare 1–100 placeholders by safe name. A placeholder named `CLIENT` uses the exact token `{{CLIENT}}`. Every declared token must already occur inside one supported text run or cell; tokens split across multiple runs or cells fail closed.
- Supported semantic locations are DOCX main-document/header/footer `w:t` runs, PPTX visible slide and speaker-notes `a:t` runs, and XLSX shared-string or inline-string `t` cells. PDF and CSV remain immutable copy templates and reject semantic placeholder declarations.
- Each placeholder may declare a description, `required` flag, optional default, and `max_length` up to 100000 characters. Names must match `[A-Za-z][A-Za-z0-9_]{0,63}` and must be unique.
- Call `inspect_artifact_template` before reuse. It verifies the stored artifact hash and rescans every declared placeholder occurrence; manifest count drift or artifact tampering is reported as invalid.
- Call `instantiate_artifact_template` with a distinct output path and a `values` object. Unknown values, missing required values, over-limit values, XML-incompatible control characters, source/target aliasing, symlinks, unsafe ZIP entries, and package/XML size violations fail before output is published.
- Semantic replacement is exact and non-recursive. A value containing another placeholder token remains literal data. Replacement never crosses text runs or cells and preserves surrounding run/cell styles plus every unrelated ZIP part through raw compressed copy.
- Legacy schema-v1 templates remain readable and instantiate as verified immutable copies. Overwriting a template directory or output artifact requires explicit `overwrite=true`.
- This bounded version does not infer placeholders automatically, merge rich text split across runs, replace image/chart/SmartArt/formula content, render artifacts, or perform visual QA. All work happens locally without launching Office, LibreOffice, Keynote, or a cloud document service.
