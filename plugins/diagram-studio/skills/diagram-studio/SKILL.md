---
name: diagram-studio
description: Use Diagram Studio MCP tools to inspect, create, revise, validate, and export editable technical diagrams. Routes every AI-generated diagram through the matching on-demand diagram guide and prevents unrelated business concerns from being crowded onto one canvas.
metadata:
  chatos.role: router
  chatos.related-skills: "diagram-architecture,diagram-flowchart,diagram-swimlane,diagram-topology,diagram-sequence"
---

# Diagram Studio MCP guide

Use Diagram Studio when the requested deliverable is an editable architecture diagram, flowchart, swimlane diagram, topology map, or sequence diagram. A Mermaid or PlantUML block in chat is not a completed Diagram Studio deliverable.

## Runtime scope

ChatOS injects the current user, tenant, project or public scope, workspace, and runtime session. Treat that context as authoritative.

- Never ask for, invent, or pass a ChatOS project ID.
- Diagram creation tools resolve the destination from the injected runtime scope.
- A conversation without a ChatOS project uses that user's isolated public Diagram Studio scope. Public does not mean shared between users.
- `artifactKey` identifies one logical diagram inside the current injected scope. It is not a project ID.
- Reuse the same `artifactKey` when revising the same deliverable. Use a new key only for an intentionally different diagram.

## First decide the diagram set

A Diagram Studio workspace may contain many diagrams. One canvas is not a complete software specification. Each diagram must answer one clear question for one audience at one level of detail.

Before writing anything, identify:

1. The question the user should answer after reading the diagram.
2. The appropriate diagram kind.
3. The viewpoint or scenario.
4. The information that must be visible.
5. The information deliberately excluded.
6. Whether independent concerns require separate diagrams.

Split the work when:

- the title needs “and”, “与”, “以及”, or “同时” to join unrelated subjects;
- an architecture overview also expands controllers, services, repositories, tables, or deployment nodes;
- a flowchart contains several unrelated business outcomes or start/end paths;
- a sequence diagram combines login, ordering, payment, refund, notification, and administration scenarios;
- a swimlane diagram attempts to cover an organization's entire operating model;
- a topology diagram mixes logical code structure with every physical deployment detail;
- the graph can fit only by shrinking text, creating a very long canvas, or accepting a mass of crossing edges.

Do not include information merely because it is true. Include it only when it helps the current diagram answer its single question.

Bad deliverable:

```text
WMS complete diagram
  every page + every controller + every service + every table
  + procurement + inventory + production + deployment + all calls
```

Better deliverable set:

```text
WMS diagrams
  system boundary overview
  order-to-production business flow
  procurement inbound flow
  inventory reservation sequence
  inventory-service internal architecture
  production deployment topology
```

## Mandatory generation protocol

For every AI-generated or structurally rewritten diagram:

1. Call `diagram_list_documents` to avoid accidental duplicates when appropriate.
2. Choose exactly one diagram kind and one mode for the next document.
3. Activate the matching dedicated Skill from the platform catalog with `skill_skill_activate`, using this router activation as `parent_activation_ref`.
4. Read the dedicated Skill's linked examples or contract resource when its instructions require them.
5. Call `diagram_prepare_generation` with the router and leaf activation evidence plus a bounded plan.
6. Call `diagram_commit_generation` with the same current Skill evidence and the returned `generationPermit`.
7. If the result is not `ready`, revise the plan or split the diagram and obtain a new permit.
8. Call `diagram_validate` before reporting completion.
9. Return the document ID, artifact key, guide ID/version, and validation result.

Activation evidence proves that the correct immutable Skill instructions were loaded in this Runtime Session. The structured plan and final quality validation prove that the instructions were applied.

## Tool reference

### `diagram_list_documents`

Lists diagrams in the current injected scope. Use it before creation when an equivalent logical deliverable may already exist. It takes no project ID. Match existing work primarily by `artifactKey`, then by title and kind.

### `diagram_list_projects`

Lists Diagram Studio's UI classification projects inside the already injected ChatOS scope. These are folders for organizing diagrams, not ChatOS project selectors. Never use this tool to choose or change the outer ChatOS project.

### `diagram_create_project`

Creates an optional UI classification folder inside the current scope. It is not required before creating a diagram. Generated diagrams are automatically assigned to the stable scope project when no UI classification move is requested.

### `diagram_get_project`, `diagram_update_project`, and `diagram_delete_project`

Read or manage UI classification metadata only. A project ID returned by these tools is internal to Diagram Studio and valid only inside the current injected scope. It must never be treated as or copied into a ChatOS project ID.

### `diagram_move_document`

Moves a document between Diagram Studio UI classifications within the current injected scope. It cannot move content across ChatOS users, projects, workspaces, tenants, or public scope. Do not use it to simulate scope switching.

### `diagram_get_document`

Returns the complete editable document. Call it before revising an existing diagram so node IDs, edge IDs, current revision, user positions, styles, and evidence can be preserved.

### `diagram_prepare_generation`

Submits the plan for one logical diagram and returns a signed `generationPermit`. The plan must state scope, excluded details, estimated size, boundaries or participants, split decisions, and the dedicated Skill checklist acknowledgements. Pass `kind`, optional `mode`, and the router plus leaf activation evidence. It takes no project ID.

A permit is bound to the injected runtime scope, diagram kind, mode, artifact key, operation, plan, and current guide version. Do not reuse it for another diagram.

### `diagram_commit_generation`

Creates or upserts one generated diagram from PlantUML. It requires a valid permit. Use stable ASCII aliases, pass source evidence for code-derived nodes, and use the same `artifactKey` and `idempotencyKey` selected in the plan.

This tool performs parsing, layout, contract checks, quality validation, persistence, and generation-provenance recording. Do not call `diagram_create_document` first.

### `diagram_import_plantuml`

Imports PlantUML through the same generation gates as `diagram_commit_generation`. It requires current Skill evidence and the matching `generationPermit`, accepts no project ID, and applies the same contract, quality, scope, and provenance checks.

### `diagram_create_document`

Creates an empty canvas only. Use it when the user explicitly wants to draw manually from a blank page. It is not preparation for generated PlantUML and it must not be followed by a second create call for the same deliverable.

### `diagram_apply_patch`

Applies focused changes to an existing document using optimistic revision control. Title, description, position, and viewport-only changes do not require a generation permit. Adding/removing nodes or semantic edges requires a permit for that document and diagram kind.

Preserve unrelated user edits. On a revision conflict, reread the document and rebase the intended patch.

### `diagram_replace_document`

Replaces the complete structured document and therefore requires a generation permit. Prefer a focused patch unless the entire semantic structure truly must change.

### `diagram_auto_layout`

Rearranges the existing graph without changing its semantic elements. Use after substantial structural edits, not after every small style or position adjustment. Do not relayout user-positioned diagrams unless requested or necessary to repair overlaps.

### `diagram_validate`

Checks structural validity and delivery readiness. Completion requires `valid: true` and `ready: true`. Blocking issues must be repaired, simplified, or split. Use the same quality profile selected by the dedicated guide.

### `diagram_export`

Exports the current document as JSON, SVG, or PlantUML. Export does not replace validation and does not create a new logical diagram.

### `diagram_delete_document`

Deletes one document in the current injected scope. Use only when the user explicitly requests deletion. Do not delete a crowded diagram merely because a better replacement was generated.

## Plan requirements

Every generation plan must contain:

- one-sentence `goal`;
- bounded `scope`;
- `excludedDetails` with at least one meaningful exclusion;
- `estimatedPrimaryItemCount` and `estimatedEdgeCount`;
- `structure`, listing the important boundaries, lanes, deployment groups, or participants;
- `splitPlan`, even when empty, with a reason for keeping or separating concerns;
- every checklist ID returned by the dedicated guide.

If the estimated size exceeds the guide contract, split the diagram instead of asking for a larger canvas.

## Evidence and completion

For code-derived diagrams, map PlantUML aliases to concrete source references. Do not place file paths in visible labels. A final response must name the created or updated documents and must not claim success if any required diagram is missing, unvalidated, or `ready: false`.
