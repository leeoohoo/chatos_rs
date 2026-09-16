# Workspace document shapes

Use complete objects for each upsert. The service owns workspace-level revision and timestamps; still include the document header fields shown below because each document is independently versioned.

The workspace envelope may contain a server-owned `hostProject` object with `projectId`, `projectName`, `connectorWorkspaceId`, and `contextScopeId`. Do not send or edit this object in upsert payloads. ChatOS injects the active project context at runtime and the service stamps the binding automatically. A Solution Studio `workspaceId` identifies the planning document and is unrelated to the host `connectorWorkspaceId`.

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
  "items": [
    { "id": "R-001", "title": "", "description": "", "priority": "must", "acceptanceCriteria": [], "evidenceIds": ["E-001"], "selectedDesignSectionId": "D-001" },
    { "id": "R-002", "parentRequirementId": "R-001", "title": "", "description": "", "priority": "must", "acceptanceCriteria": [], "evidenceIds": ["E-001"], "selectedDesignSectionId": "D-002" }
  ],
  "updatedAt": "ISO-8601 timestamp"
}
```

Allowed statuses are `draft`, `review`, and `approved`. Evidence confidence is `verified`, `user-stated`, or `assumption`. Requirement priority is shown to users as `高`, `中`, or `低`; encode these as `must`, `should`, or `could` respectively.

The workspace-level `projectProfile` contains `background`, `overview`, `projectType`, `deliveryForm`, and `targetPlatforms`. It describes the project, not a requirement. `parentRequirementId` creates a requirement tree; omit it for a top-level requirement.

## SolutionDesignDocument

```json
{
  "status": "draft",
  "revision": 0,
  "basedOnRequirementsRevision": 1,
  "summary": "",
  "blocks": [
    { "id": "D-000-B-001", "type": "text", "title": "项目技术基线", "content": "Language, framework, UI, architecture, data, testing, build, and deployment choices" },
    { "id": "D-000-B-002", "type": "architecture", "title": "总体技术架构", "content": "<svg viewBox=\"0 0 1200 800\">...</svg>" }
  ],
  "sections": [
    {
      "id": "D-001",
      "title": "",
      "body": "Short candidate-solution summary",
      "requirementIds": ["R-001"],
      "evidenceIds": ["E-001"],
      "blocks": [
        { "id": "D-001-B-001", "type": "text", "title": "方案说明", "content": "Markdown" },
        { "id": "D-001-B-002", "type": "architecture", "title": "系统架构", "content": "<svg viewBox=\"0 0 1200 800\">...</svg>" },
        { "id": "D-001-B-003", "type": "flowchart", "title": "关键流程", "content": "<svg viewBox=\"0 0 1200 800\">...</svg>" },
        { "id": "D-001-B-004", "type": "ui-svg", "title": "页面设计", "content": "<svg viewBox=\"0 0 1440 900\">...</svg>" }
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

Top-level `design.blocks` contain the decided project technical baseline and overall architecture. Each requirement maps to exactly one `DesignSection`; its `blocks` contain the requirement-specific detailed design. `text` contains Markdown; `architecture`, `flowchart`, and `ui-svg` contain standalone SVG source without Markdown fences. `selectedDesignSectionId` links the requirement to its single design section for compatibility and traceability; it is not a user choice among alternatives.

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
  "positions": {},
  "viewport": { "x": 0, "y": 0, "zoom": 1 },
  "updatedAt": "ISO-8601 timestamp"
}
```

Task type is `task`, `review`, or `milestone`. Task status is `planned`, `in_progress`, `blocked`, `done`, or `cancelled`; a blocked task may include `blockedReason`. `dependsOn` contains prerequisite task IDs. `positions` maps task IDs to `{ "x": number, "y": number }` and is presentation-only.
