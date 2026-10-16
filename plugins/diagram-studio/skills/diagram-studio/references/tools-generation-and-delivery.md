# Tools, generation plans, and delivery

Read this reference only when choosing Diagram Studio calls, preparing a generation plan, revising an existing document, or delivering an export.

## Tool selection

- `diagram_list_documents`: list diagrams in the injected scope before creation when an equivalent deliverable may already exist. Match by stable `artifactKey`, then title and kind.
- `diagram_list_projects`: list Diagram Studio UI folders inside the injected ChatOS scope. These are not ChatOS project selectors.
- `diagram_create_project`, `diagram_get_project`, `diagram_update_project`, `diagram_delete_project`, `diagram_move_document`: manage only Diagram Studio UI classification metadata. Never use them to change outer ChatOS scope.
- `diagram_get_document`: read the complete editable document before revision so IDs, current revision, user positions, styles, and evidence can be preserved.
- `diagram_prepare_generation`: store the plan for one logical diagram. Pass `kind`, optional `mode`, and stable `artifactKey`; do not pass an operation, document ID, permit, activation field, project ID, or host context. The plugin resolves create versus revise.
- `diagram_commit_generation`: create or upsert generated PlantUML using the active plan. Use the same kind, title, artifact key, and idempotency key. Do not create an empty document first.
- `diagram_import_plantuml`: import through the same generation, scope, quality, and provenance gates as commit.
- `diagram_create_document`: create a blank manual canvas only when the user explicitly wants to draw manually.
- `diagram_apply_patch`: apply focused optimistic-revision changes. Position, title, description, and viewport changes do not require a plan; semantic node or edge changes do.
- `diagram_replace_document`: replace all structured content and therefore requires an active plan. Prefer a focused patch.
- `diagram_auto_layout`: rearrange existing elements without semantic changes. Do not relayout user-positioned work unless requested or needed to repair overlaps.
- `diagram_validate`: completion requires both `valid: true` and `ready: true`.
- `diagram_export`: export JSON, SVG, or PlantUML after validation. Export does not create a new logical diagram.
- `diagram_delete_document`: delete only on the user's explicit request.

## Generation plan contract

Every plan must contain:

- one-sentence `goal`;
- bounded `scope`;
- `excludedDetails` with at least one meaningful exclusion;
- `estimatedPrimaryItemCount` and `estimatedEdgeCount`;
- `structure` listing important boundaries, lanes, deployment groups, or participants;
- `splitPlan`, even when empty, with a reason for keeping or separating concerns;
- every checklist ID from the activated dedicated diagram guide.

If the estimate exceeds that guide's bounds, split the work instead of asking for a larger canvas. The same `artifactKey` represents one logical diagram across revisions; use a new key only for a distinct deliverable. Never invent placeholder IDs such as `create`, `none`, or `new`.

## Evidence and completion

For code-derived diagrams, map PlantUML aliases to concrete source references in evidence metadata and keep file paths out of visible labels. Preserve unrelated user edits during revision; on conflict, reread and rebase. Report the created or updated documents, artifact keys, guide IDs and versions, validation results, and export files. Do not claim completion while a required diagram is missing, unvalidated, or not ready.
