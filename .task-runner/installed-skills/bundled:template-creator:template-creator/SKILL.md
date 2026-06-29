---
name: template-creator
description: Create or update a reusable personal Codex artifact-template skill. Use when the user invokes $template-creator or asks in natural language to create a template using, from, or based on an attached Word document, PowerPoint presentation, or Excel workbook, or explicitly asks to edit or update a passed artifact-template skill. Do not use for one-off artifact creation from an existing template.
---

# Template Creator

Create or update a reference-backed artifact template. Keep the source Office file inside the skill so later use can clone or import it precisely.

## Routing

- Manage only personal skills under `${CODEX_HOME:-~/.codex}/skills`.
- Create a new template by default. Use a numbered skill name instead of overwriting an existing template.
- Update only when the user explicitly asks to edit or update exactly one passed artifact-template skill. Treat that passed skill as the exact target; never choose a similarly named template.
- Do not modify an installed or bundled plugin cache. If the passed template is plugin-backed, explain that this skill can update only a personal template.
- Do not create, modify, upload, or publish a plugin. If the request also asks to share the template with a workspace, explain that this skill only manages personal templates.

## Create workflow

1. Require exactly one `.docx`, `.pptx`, or `.xlsx` reference unless the user explicitly requests a batch. For a batch, complete this workflow separately for every file.
2. Infer a concise display name, intended-use description, and artifact kind from the reference and request.
3. Create `preview.png` before packaging:
   - DOCX: use Documents to render the reference and copy its first page PNG.
   - PPTX: use Presentations to render the reference and copy its first slide PNG.
   - XLSX: use Spreadsheets to render the used range of the first visible non-empty sheet.
4. Visually inspect the PNG. Stop if it is blank, clipped, corrupted, or not representative of the reference.
5. Write a JSON request in a temporary directory with this shape:

```json
{
  "referencePath": "/absolute/path/reference.docx",
  "previewPath": "/absolute/path/preview.png",
  "title": "Standup",
  "description": "Run a structured daily standup with updates, blockers, and owners."
}
```

6. Set `SKILL_DIR` to the directory containing this `SKILL.md`, load the workspace dependency runtime, and run:

```bash
"$NODE_BIN" "$SKILL_DIR/scripts/create-template-skill.mjs" --request "/absolute/path/request.json"
```

Use the Node path returned by the dependency loader for `NODE_BIN`. Do not use a system Node installation.

7. Read the JSON result. Verify that the generated directory contains `SKILL.md`, `artifact-template.json`, `agents/openai.yaml`, the retained `assets/reference.<ext>`, and `assets/preview.png`.

## Update workflow

1. Resolve the exact passed artifact-template skill and read its `SKILL.md`, `artifact-template.json`, `agents/openai.yaml`, retained reference, and preview. Stop if it is not a direct child of the personal skills directory or if more than one target was passed.
2. Preserve the skill folder name and every file or behavior the user did not ask to change.
3. Apply the requested edit:
   - For reference content or visual changes, use the matching artifact plugin to edit a temporary copy of the retained reference, render a new preview from it, and visually inspect the result.
   - For title or intended-use changes, preserve the current reference and preview unless the request also changes them.
   - For instruction-only or other skill-owned text changes, edit only the requested files directly and keep the manifest and agent metadata consistent.
4. When the reference, preview, title, or description changes, write an update request using the existing values for every unchanged field:

```json
{
  "mode": "update",
  "templateName": "artifact-template-standup",
  "referencePath": "/absolute/path/updated-reference.docx",
  "previewPath": "/absolute/path/updated-preview.png",
  "title": "Standup",
  "description": "Run a structured daily standup with updates, blockers, and owners."
}
```

5. Run the same script command from the create workflow. It validates the existing template kind, preserves additional skill-owned files, and replaces the package atomically without changing its skill name.
6. Verify every requested change in the target directory and confirm there are no staging or backup directories left behind.

## Response

After verification, say `Created` for a new template or `Updated` for an edited template, replacing the example values with the result:

Created the personal template $artifact-template-artifacts-meeting-notes.

- Location: [~/.codex/skills/artifact-template-artifacts-meeting-notes](/absolute/path/to/.codex/skills/artifact-template-artifacts-meeting-notes)
- Artifact type: Document

Formatting rules:

- Emit the returned `skillName` as an unquoted `$skill-name` mention so Codex renders the skill link UI with its display name. Do not wrap the mention in backticks or make it a Markdown link.
- Make the location label the user-facing skill directory and link it to the exact absolute `skillPath` returned by the script. The link target must be an absolute local path; use `~` only in the label when applicable.
- Capitalize the returned artifact kind as `Document`, `Presentation`, or `Spreadsheet`.
- For a batch, repeat the three-line block for each created or updated skill.

## Constraints

- Do not search for or fetch remote templates.
- Do not delete or sanitize the retained reference; the user chose reference retention for fidelity.
- Do not create or mutate workspace plugins or marketplaces.
- Do not add Artifact.md package generation here. The artifact plugins own template distillation and creation.
- Do not modify global skill metadata or protocol files.
