# Workspace schema v2

Use complete objects for each authoring upsert. The active ChatOS project owns exactly one v2 workspace envelope: one `projectProfile`, one `RequirementsDocument`, one `SolutionDesignDocument`, and one `ExecutionPlan`. The service owns workspace revision, timestamps, project binding, and execution status history.

There is no legacy conversion path. Do not send a v1 object, omit v2 fields, reconstruct deleted legacy data, or try to preserve an old status representation. Invalid legacy workspace files are discarded.

The server-owned envelope includes `schemaVersion: 2`, `workspaceId`, `revision`, timestamps, and optionally `hostProject`. Do not send or edit `hostProject`. ChatOS injects active project context. Store all planning text and SVG in the workspace, never under the host project root.

## RequirementsDocument

```json
{
  "status": "draft",
  "revision": 0,
  "summary": "",
  "goals": [],
  "users": [],
  "inScope": [],
  "outOfScope": [],
  "constraints": [],
  "assumptions": [],
  "openQuestions": [],
  "evidence": [
    { "id": "E-001", "label": "", "source": "path, symbol, document, or user brief", "note": "", "confidence": "verified" }
  ],
  "modules": [
    { "id": "MOD-CORE", "title": "Core business", "kind": "business", "description": "Ownership and boundary", "order": 0 }
  ],
  "items": [
    { "id": "R-001", "moduleId": "MOD-CORE", "kind": "capability", "title": "", "description": "", "priority": "must", "acceptanceCriteria": [], "evidenceIds": ["E-001"], "selectedDesignSectionId": "D-001" },
    { "id": "R-002", "moduleId": "MOD-CORE", "kind": "quality", "parentRequirementId": "R-001", "title": "", "description": "", "priority": "should", "acceptanceCriteria": [], "evidenceIds": ["E-001"], "selectedDesignSectionId": "D-002" }
  ],
  "updatedAt": "ISO-8601 timestamp"
}
```

Module kind is `foundation`, `business`, or `delivery`. Requirement kind is `capability`, `quality`, or `constraint`. A software project needs at least one business module. Every requirement references an existing module; parent and child requirements must share the same module. Modules are functional ownership boundaries, not schedule phases.

Document status is `draft`, `review`, or `approved`. Evidence confidence is `verified`, `user-stated`, or `assumption`. Requirement priority is shown as `高`, `中`, or `低` and encoded as `must`, `should`, or `could`.

The workspace-level `projectProfile` contains `background`, `overview`, `projectType`, `deliveryForm`, and `targetPlatforms`. `solution_upsert_requirements` also receives the workspace `description`.

## SolutionDesignDocument

```json
{
  "status": "draft",
  "revision": 0,
  "basedOnRequirementsRevision": 1,
  "summary": "",
  "blocks": [
    { "id": "D-000-B-001", "type": "text", "title": "Project technical baseline", "content": "Concrete technology and delivery choices" },
    { "id": "D-000-B-002", "type": "architecture", "title": "Overall architecture", "content": "<svg viewBox=\"0 0 1200 800\">...</svg>" }
  ],
  "sections": [
    {
      "id": "D-001",
      "moduleId": "MOD-CORE",
      "title": "",
      "body": "Short decided-solution summary",
      "requirementIds": ["R-001"],
      "evidenceIds": ["E-001"],
      "blocks": [
        { "id": "D-001-B-001", "type": "text", "title": "Detailed design", "content": "Markdown" },
        { "id": "D-001-B-002", "type": "flowchart", "title": "Key flow", "content": "<svg viewBox=\"0 0 1200 800\">...</svg>" }
      ]
    }
  ],
  "decisions": [
    { "id": "ADR-001", "title": "", "decision": "", "rationale": "", "alternatives": [], "consequences": [], "requirementIds": ["R-001"] }
  ],
  "risks": [],
  "validationStrategy": [],
  "updatedAt": "ISO-8601 timestamp"
}
```

Top-level `blocks` contain the overall approach. Every requirement maps to exactly one decided `DesignSection` in the same module. `text` contains Markdown; `architecture`, `flowchart`, and `ui-svg` contain standalone inert SVG with a `viewBox`. Alternatives belong in `decisions`, not parallel sections.

## ExecutionPlan

```json
{
  "status": "draft",
  "revision": 0,
  "basedOnDesignRevision": 1,
  "objective": "",
  "tasks": [
    {
      "id": "T-001",
      "moduleId": "MOD-CORE",
      "title": "",
      "description": "",
      "type": "task",
      "phase": "",
      "dependsOn": [],
      "status": "planned",
      "requirementIds": ["R-001"],
      "designSectionIds": ["D-001"],
      "deliverables": [],
      "acceptanceCriteria": [],
      "sourceReferences": []
    }
  ],
  "statusEvents": [],
  "positions": {},
  "viewport": { "x": 0, "y": 0, "zoom": 1 },
  "updatedAt": "ISO-8601 timestamp"
}
```

Task type is `task`, `review`, or `milestone`. New authoring tasks must use status `planned`. `dependsOn` contains only direct prerequisites and must be acyclic and transitively reduced. Cross-module prerequisites are allowed and retain their original owning module.

The authoring component must preserve existing task statuses and `statusEvents`. It cannot modify or delete a task after that task leaves `planned`. The executor exclusively performs these transitions:

- `planned -> in_progress` with `solution_claim_task`, after all prerequisites are `done`;
- `blocked -> in_progress` with `solution_resume_task`, after prerequisites remain `done`;
- `in_progress -> blocked` with a reason;
- `in_progress -> done` with a completion summary and optional evidence;
- `planned | in_progress | blocked -> cancelled` with a reason.

`done` and `cancelled` are terminal. They cannot be reopened. Each transition appends an immutable event containing `taskId`, `from`, `to`, `occurredAt`, `runtimeSessionId`, optional reason/summary, and evidence. `positions` and `viewport` are presentation state only and never define execution order.
