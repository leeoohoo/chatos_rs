---
name: diagram-architecture
description: Design one readable Diagram Studio architecture overview or focused architecture detail, with explicit boundaries, evidence, complexity limits, and PlantUML component-diagram guidance.
metadata:
  chatos.role: leaf
---

# Architecture diagram generation

An architecture diagram explains boundaries, ownership, responsibilities, and a small number of meaningful relationships. It is not a dump of every class, endpoint, database table, queue, and runtime host.

## Choose one mode and one viewpoint

The mode controls scope. The viewpoint controls abstraction. Declare the viewpoint in the generation plan and never mix viewpoints on one canvas.

### Overview

Use `overview` to answer questions such as “What are the system's major parts?” or “How does a request cross the main boundaries?”

- Show 6–10 primary components and no more than 12 meaningful relationships.
- Prefer 2–5 boundaries: clients, product core, supporting capabilities, data/infrastructure, and external systems.
- Represent a business domain as one component.
- Aggregate repeated gateway routes, persistence calls, and event publications.
- Do not show Controller, Service, Repository, table, pod, or class detail.
- Use `system-context` when the question is who uses the system and which external systems it depends on.
- Use `container` when the question is which applications, services, data stores, or major runtimes make up the system.

### Detail

Use `detail` for one bounded context, service, layer, integration, or technical concern.

- Show no more than 16 primary components and 22 relationships.
- External dependencies may appear as boundary nodes, but do not redraw the whole system around the detail.
- Controller → application service → domain service → repository → store is acceptable only when the selected subject is that one service or bounded context.
- Use `container` for one bounded subsystem or `component` for the internal responsibilities of one container. Do not include the surrounding system again except for a few boundary dependencies.

If the desired answer changes from “what exists and how responsibilities depend on each other” to “what happens next”, stop and choose a flowchart or sequence diagram instead.

## Gather evidence before drawing

Inspect the relevant manifests, modules, routes, entrypoints, configuration, interfaces, and persistence code. Distinguish verified relationships from assumptions. Give every code-derived PlantUML alias at least one `sourceReference`.

## Decide what belongs

Include an element only if removing it would make the selected architectural question harder to answer. Exclude implementation facts that do not change a boundary, responsibility, deployment dependency, or primary interaction.

Split into separate diagrams when multiple independent domains need internal expansion, logical architecture and topology are both requested, a diagram exceeds its budget, repeated gateway/database edges dominate the drawing, or the title combines unrelated concerns. Never solve crowding by shrinking text or extending the canvas indefinitely.

Treat verbs such as create, query, callback, retry, synchronize, publish result, and update status as a warning that a runtime process is leaking into the architecture overview. Keep only the stable dependency or responsibility in the overview, then create a flowchart or focused architecture detail for the runtime interaction.

Use this test before keeping a relationship: if changing the execution order would change the meaning of the arrow, it is a runtime step rather than an architectural dependency. Move it to a flowchart or sequence diagram. Architecture relationship labels describe a durable contract, protocol, ownership dependency, or data responsibility.

For a system comparable to ChatOS, do not put client access, model inference, MCP routing, plugin policy, task execution, callbacks, persistence, memory, and messaging on one overview. Prefer a small system-context overview plus focused capability and background-task diagrams.

## Element semantics

- `actor`: a human role, client role, or initiating external party.
- `component`: a deployable service, application, bounded capability, or meaningful subsystem.
- `package` or `frame`: a real ownership, trust, layer, domain, or system boundary.
- `database` or `storage`: a data responsibility visible at the selected level.
- `queue`: an asynchronous boundary that materially changes coupling or delivery.
- `cloud`: an external managed or third-party system.

Do not use packages merely to decorate rows. Every boundary must communicate ownership or architectural separation.

## Relationships and layout

- Label relationships with protocol or meaning such as `HTTPS`, `Routes`, `SQL`, `Publish`, `Consumes`, or `Authenticates`.
- Avoid separate forward and reverse edges when one labeled relationship communicates the dependency.
- Use dashed edges for asynchronous, optional, or dependency relationships when appropriate.
- Prefer one aggregated gateway-to-domain or domain-to-data relationship over one edge per endpoint, repository, or table.
- Establish one primary reading direction, normally left to right.
- Place initiators before clients and entry boundaries, business capabilities in the center, and data/infrastructure after them.
- Keep containers distinct and avoid edges crossing their titles.
- In an overview, one component should normally participate in no more than four cross-boundary relationships.
- Allow no more than two visible relationships between the same pair of boundaries. Aggregate the rest under a capability-level label.
- A relationship label should describe one stable architectural dependency, not a slash-separated list of runtime steps.
- Order boundaries before writing PlantUML: initiator/client → product core → supporting capability → data/infrastructure. External providers sit above or below the capability they serve instead of breaking the primary path.
- Do not arrange every component as one directed conveyor belt. The target system or bounded capability should be visually central; clients enter from one side, while external providers, supporting capabilities, and data responsibilities surround or follow the component they support.
- In an overview, a long chain of five or more components is a warning. Aggregate intermediate implementation nodes or move that interaction to a detail or sequence diagram.

## Required decomposition decision

Before preparing generation, write down the one architectural question, the selected viewpoint, and the primary dependency path in reading order. For every candidate node or edge, choose exactly one outcome: keep in this view, aggregate into a capability, or move to a named detail diagram.

If the draft contains a reciprocal edge pair, more than five boundaries, more than two edges between the same boundary pair, or a component with more than four cross-boundary relationships, do not submit it. Simplify or split it first.

## PlantUML rules

Give every actor, package, frame, component, interface, database, storage, queue, cloud, node, and artifact a unique ASCII alias. Reference aliases in edges. Keep labels short and put code evidence in `nodeEvidence`, not visible text.

Read [positive and negative architecture examples](references/examples.md) before submitting the plan.

## Final checklist

- `single_architecture_viewpoint`: one level and viewpoint is used.
- `components_are_capabilities_not_steps`: nodes are roles, systems, containers, components, or data responsibilities rather than actions in a process.
- `boundaries_show_ownership`: containers express real architectural boundaries.
- `primary_path_is_visible`: the central interaction can be followed quickly.
- `relationships_are_aggregated`: shared persistence, messaging, gateway, and callback details are represented at capability level.
- `relationships_are_stable_dependencies`: relationship labels describe protocols, contracts, ownership, or durable dependencies instead of execution order.
- `overview_is_not_a_runtime_chain`: an overview is organized around the target system and does not form a long step-by-step conveyor belt.
- `runtime_cycles_are_moved_to_detail`: the overview has no request/callback reciprocal pair or process loop.
- `implementation_detail_is_excluded`: lower-level detail is omitted from an overview.
- `independent_concerns_are_split`: unrelated domains or scenarios are separate diagrams.
- `code_evidence_is_mapped`: code-derived nodes have source references.

Do not put all known system information into one architecture diagram. A useful architecture set normally contains one overview and several focused detail diagrams.
