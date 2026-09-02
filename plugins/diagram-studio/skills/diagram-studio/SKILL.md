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
5. Prefer `diagram_apply_patch` over complete replacement so unrelated user edits are preserved.
6. If a revision conflict occurs, read the document again and rebase the intended change.
7. Run `diagram_auto_layout` after adding or removing several nodes.
8. Run `diagram_validate` before reporting that a project-derived diagram is complete.
9. Use edge labels for protocols or semantics such as `HTTPS`, `MCP`, `SQL`, `Publish`, `Consume`, or `depends_on`.
10. Do not invent code relationships. If a relationship is inferred rather than verified, state that in the node or edge description.
11. For diagrams supplied as PlantUML, use `diagram_import_plantuml` so the visual editor and source share the same semantic model. Pass `kind` when the source is ambiguous.
12. Preserve user-authored canvas layout and styling. Use focused document patches for visual changes; do not replace an existing diagram from generated PlantUML unless the user asks to apply source changes.

## Diagram choice

- Architecture: services, modules, storage, APIs, queues, and external systems.
- Flowchart: sequential steps, decisions, branches, retries, and terminal states.
- Swimlane: cross-team or cross-system workflows with ownership boundaries.
- Topology: hosts, zones, networks, gateways, clusters, replicas, and observability.
- Sequence: participants, synchronous calls, return messages, activation intervals, and combined fragments.

## Typical sequence

1. Inspect the project or process description.
2. Call `diagram_create_document` with the closest diagram kind.
3. Read the created document.
4. Patch nodes and edges with verified names and evidence.
5. Auto-layout.
6. Validate.
7. Export JSON, SVG, or PlantUML as requested.

## PlantUML workflow

- Sequence supports participant, actor, boundary, control, entity, database, queue, messages, activate/deactivate, and alt/opt/loop-style fragments.
- Flowchart supports Activity Diagram `start`, activities, `if/else/endif`, and `stop` semantics.
- Swimlane supports the same Activity semantics plus `partition` and `|Lane|` lane switches.
- Architecture supports Component Diagram actors, components, interfaces, databases, queues, and labeled or dashed dependencies.
- Topology supports Deployment Diagram nodes, clouds, databases, storage, artifacts, and labeled or dashed infrastructure links.
- Diagram Studio layout comments are valid PlantUML comments. Keep them intact when exact canvas round-tripping matters.
- Unknown PlantUML statements are preserved as opaque source blocks but may not have visual editing controls.
