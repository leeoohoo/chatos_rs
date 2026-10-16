---
name: diagram-studio
description: Use Diagram Studio MCP tools to inspect, create, revise, validate, and export editable technical diagrams. Routes every AI-generated diagram through the matching on-demand diagram guide and prevents unrelated business concerns from being crowded onto one canvas.
metadata:
  chatos.role: router
  chatos.related-skills: "diagram-architecture,diagram-flowchart,diagram-swimlane,diagram-topology,diagram-sequence,diagram-mindmap"
---

# Diagram Studio MCP guide

Use Diagram Studio when the requested deliverable is an editable architecture diagram, flowchart, swimlane diagram, topology map, sequence diagram, or mind map. A Mermaid or PlantUML block in chat is not a completed Diagram Studio deliverable.

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
- a mind map mixes unrelated subjects, exceeds four useful levels, repeats topics under several parents, or turns labels into paragraphs;
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
3. Activate the matching dedicated Skill using its exact platform catalog `skill_ref`. Main Chat and Task Execution use `agent_skill_activate`, whose result already lists declared references; read only a reference needed for the current diagram with `agent_skill_read_resource`. If that activation result is no longer available after resume, activate the same exact ref again. Other host surfaces use their advertised Skill tools and schemas. ChatOS associates the leaf with this router and manages activation evidence internally; never invent evidence or call an unadvertised alias.
4. Read only the dedicated Skill examples its instructions require. Package-internal contracts and UI metadata are not model references.
5. Call `diagram_prepare_generation` with a stable `artifactKey` and a bounded plan. Do not pass an operation, document ID, permit, activation field, or host context; the plugin stores the approved plan internally and resolves create versus revise inside the injected scope.
6. Call `diagram_commit_generation` with the PlantUML source and the same semantic `kind`, `title`, and `artifactKey`.
7. If the result is not `ready`, revise the plan or split the diagram and prepare it again.
8. Call `diagram_validate` before reporting completion.
9. Return the document ID, artifact key, guide ID/version, and validation result.

The platform's internal activation graph proves that the correct immutable Skill instructions were loaded in this Runtime Session. The structured plan and final quality validation prove that the instructions were applied. AI-facing tool arguments contain business content only.

## Load operational details only when needed

Before selecting calls, preparing a plan, revising an existing document, or delivering an export, read [tools, generation plans, and delivery](references/tools-generation-and-delivery.md). Do not load it merely to decide whether Diagram Studio is the right capability.
