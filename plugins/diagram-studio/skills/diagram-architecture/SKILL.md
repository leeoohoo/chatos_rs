---
name: diagram-architecture
description: Design one readable Diagram Studio architecture overview or focused architecture detail, with explicit boundaries, evidence, complexity limits, and PlantUML component-diagram guidance.
metadata:
  chatos.role: leaf
---

# Architecture diagram generation

An architecture diagram explains boundaries, ownership, responsibilities, and a small number of meaningful relationships. It is not a dump of every class, endpoint, database table, queue, and runtime host.

## Choose one mode

### Overview

Use `overview` to answer questions such as “What are the system's major parts?” or “How does a request cross the main boundaries?”

- Show 8–12 primary components and no more than 18 meaningful relationships.
- Prefer 4–7 boundaries: users/external systems, clients, entry, business capabilities, data, and infrastructure.
- Represent a business domain as one component.
- Aggregate repeated gateway routes, persistence calls, and event publications.
- Do not show Controller, Service, Repository, table, pod, or class detail.

### Detail

Use `detail` for one bounded context, service, layer, integration, or technical concern.

- Show no more than 20 primary components and 28 relationships.
- External dependencies may appear as boundary nodes, but do not redraw the whole system around the detail.
- Controller → application service → domain service → repository → store is acceptable only when the selected subject is that one service or bounded context.

## Gather evidence before drawing

Inspect the relevant manifests, modules, routes, entrypoints, configuration, interfaces, and persistence code. Distinguish verified relationships from assumptions. Give every code-derived PlantUML alias at least one `sourceReference`.

## Decide what belongs

Include an element only if removing it would make the selected architectural question harder to answer. Exclude implementation facts that do not change a boundary, responsibility, deployment dependency, or primary interaction.

Split into separate diagrams when multiple independent domains need internal expansion, logical architecture and topology are both requested, a diagram exceeds its budget, repeated gateway/database edges dominate the drawing, or the title combines unrelated concerns. Never solve crowding by shrinking text or extending the canvas indefinitely.

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

## PlantUML rules

Give every actor, package, frame, component, interface, database, storage, queue, cloud, node, and artifact a unique ASCII alias. Reference aliases in edges. Keep labels short and put code evidence in `nodeEvidence`, not visible text.

Read [positive and negative architecture examples](references/examples.md) before submitting the plan.

## Final checklist

- `single_architecture_viewpoint`: one level and viewpoint is used.
- `boundaries_show_ownership`: containers express real architectural boundaries.
- `primary_path_is_visible`: the central interaction can be followed quickly.
- `implementation_detail_is_excluded`: lower-level detail is omitted from an overview.
- `independent_concerns_are_split`: unrelated domains or scenarios are separate diagrams.
- `code_evidence_is_mapped`: code-derived nodes have source references.

Do not put all known system information into one architecture diagram. A useful architecture set normally contains one overview and several focused detail diagrams.
