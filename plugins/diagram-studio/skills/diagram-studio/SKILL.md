---
name: diagram-studio
description: Create and edit structured architecture diagrams, flowcharts, swimlane diagrams, topology maps, and sequence diagrams, including PlantUML Sequence, Activity, Component, and Deployment import and export, with Diagram Studio MCP tools.
---

# Diagram Studio workflow

Use Diagram Studio when a user asks to visualize a software architecture, process, responsibility flow, infrastructure topology, dependency map, or interaction sequence.

## Rules

1. Read the relevant project files before claiming that a component or connection exists.
2. Use stable, descriptive node IDs such as `api-gateway`, `project-service`, or `postgres-primary`.
3. Put source paths or concise evidence into each node's `data.sourceReferences` when the diagram is based on code.
4. Read the current document and revision before editing it.
5. Treat the ChatOS runtime scope as authoritative. Never accept or invent a ChatOS project ID in tool arguments; the host injects it outside the model-controlled MCP schema.
6. List or create a Diagram Studio project before creating a diagram. Pass its `projectId` to document creation and PlantUML import tools.
7. Prefer `diagram_apply_patch` over complete replacement so unrelated user edits are preserved.
8. If a revision conflict occurs, read the document again and rebase the intended change.
9. Run `diagram_auto_layout` after adding or removing several nodes.
10. Run `diagram_validate` before reporting that a project-derived diagram is complete.
11. Use edge labels for protocols or semantics such as `HTTPS`, `MCP`, `SQL`, `Publish`, `Consume`, or `depends_on`.
12. Do not invent code relationships. If a relationship is inferred rather than verified, state that in the node or edge description.
13. For diagrams supplied as PlantUML, use `diagram_import_plantuml` so the visual editor and source share the same semantic model. Pass `kind` when the source is ambiguous.
14. Preserve user-authored canvas layout and styling. Use focused document patches for visual changes; do not replace an existing diagram from generated PlantUML unless the user asks to apply source changes.
15. Every mutating AI call must include an `idempotencyKey` that is stable for that single intended tool action. Reuse it only when the same call is retried; generate a new one for an intentional second diagram.
16. For an AI-maintained deliverable, also use a stable `artifactKey` (for example `system-architecture-main` or `checkout-sequence-v2`) with `mode: "upsert"`. This identifies one diagram instance, not a diagram category. Multiple architecture diagrams are valid when they have different artifact keys or use `create_new`.
17. Do not use a diagram title as identity. Titles may be renamed or translated; `artifactKey` is the logical instance identity inside a Diagram Studio project.

## Diagram choice

- Architecture: services, modules, storage, APIs, queues, and external systems.
- Flowchart: sequential steps, decisions, branches, retries, and terminal states.
- Swimlane: cross-team or cross-system workflows with ownership boundaries.
- Topology: hosts, zones, networks, gateways, clusters, replicas, and observability.
- Sequence: participants, synchronous calls, return messages, activation intervals, and combined fragments.

## Typical sequence

1. Inspect the project or process description.
2. Call `diagram_list_projects` in the current isolated runtime scope.
3. Reuse the intended Diagram Studio project or call `diagram_create_project`.
4. Choose a stable `artifactKey` for the intended diagram instance and an `idempotencyKey` for this tool action. Call `diagram_create_document` or `diagram_import_plantuml` with that `projectId`, both keys, and `mode: "upsert"`.
5. Read the created document.
6. Patch nodes and edges with verified names and evidence.
7. Auto-layout.
8. Validate.
9. Export JSON, SVG, or PlantUML as requested.

## Architecture diagram standard

An architecture diagram is not a flat dependency graph. It must communicate system boundaries, layers, ownership, and one primary reading direction.

- Start with a system-context or container-level view. Default to 4–7 top-level domains such as External, Client, Edge/API, Application/Domain, Data, and Infrastructure.
- Use PlantUML `package`, `frame`, or other grouped structural blocks for real visual boundaries. Put components inside their owning boundary; never represent a package as a peer component.
- Keep one clear direction, normally left-to-right: users/external systems → clients → entry points → domain services → data/infrastructure.
- Show only verified, architecturally important dependencies. Prefer one labeled edge for a meaningful protocol or responsibility; omit incidental imports and utility calls.
- Limit each boundary to roughly 3–6 core components. If the whole view exceeds about 20 components, create a high-level overview and separate detail diagrams instead of shrinking everything into one canvas.
- Avoid edges that span the whole canvas, duplicated bidirectional edges, crossing lines, floating nodes, and unrelated implementation details.
- Use readable labels: a short component name plus an optional concise responsibility. Do not place file paths, environment variables, long class lists, or source excerpts in the visible label; keep evidence in `sourceReferences`.
- Use a consistent visual hierarchy: boundaries are subdued dashed containers, components are transparent with visible borders, and external actors/data stores use appropriate icons.
- Before completion, verify that every top-level boundary has a purpose, the primary request/data path can be followed without guessing, labels remain readable at fit-to-view, and unused whitespace is not caused by an outlier node or long edge.

For project-derived architecture, prefer `diagram_import_plantuml` with stable aliases and grouped Component Diagram source. Example structure:

```plantuml
@startuml
left to right direction
actor User
package "Client" as client {
  component "Desktop App" as desktop
}
package "Application" as app {
  component "API Gateway" as gateway
  component "Core Service" as core
}
package "Data" as data {
  database "PostgreSQL" as db
}
User --> desktop : Uses
desktop --> gateway : HTTPS
gateway --> core : RPC
core --> db : SQL
@enduml
```

## PlantUML workflow

- Sequence supports participant, actor, boundary, control, entity, database, queue, messages, activate/deactivate, and alt/opt/loop-style fragments.
- Flowchart supports Activity Diagram `start`, activities, `if/else/endif`, and `stop` semantics.
- Swimlane supports the same Activity semantics plus `partition` and `|Lane|` lane switches.
- Architecture supports Component Diagram actors, components, interfaces, databases, queues, and labeled or dashed dependencies.
- Topology supports Deployment Diagram nodes, clouds, databases, storage, artifacts, and labeled or dashed infrastructure links.
- Diagram Studio layout comments are valid PlantUML comments. Keep them intact when exact canvas round-tripping matters.
- Unknown PlantUML statements are preserved as opaque source blocks but may not have visual editing controls.
