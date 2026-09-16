---
name: solution-design
description: Create or revise a traceable product and technical design from structured requirements, including text, code-generated architecture diagrams, flowcharts, and SVG interface designs. Use before implementation planning.
---

# Solution Design

Design against a concrete requirements revision. Read the current workspace first and set `basedOnRequirementsRevision` to the revision actually used. Produce one decided solution for the project, not a menu of implementation paths.

Use `design.blocks` for the project-level solution. For a development project it must contain a technical-baseline text block and an overall architecture SVG. The technical baseline must explicitly choose the language and runtime, application or game framework, UI/rendering technology, state and rules architecture, data storage or database, testing toolchain, build system, and deployment shape. State when a server database is intentionally not needed. The overall architecture must show the major runtime boundaries and data flow.

Create exactly one `DesignSection` for each requirement and set that requirement's `selectedDesignSectionId` to it. A section is the requirement's detailed design inside the fixed project architecture, not a candidate. Record rejected or considered alternatives only in `decisions[].alternatives`; never create a second design section to represent another route.

For a development project, every section needs a substantive text block. A one-sentence `body` is only the summary and never counts as the design document. Cover the relevant module responsibilities, interfaces or data model, state and key flow, error and fallback behavior, compatibility or rollout constraints, observability, and verification. Omit only headings that genuinely do not apply, not the document itself.

Read [design content types](references/design-content-types.md) before generating or revising blocks. Visual blocks contain self-contained SVG source code produced directly for this workspace. Do not call, reference, bind to, or create assets in another diagram or design plugin. Solution Studio stores the code and renders it locally.

Use only the block types that make the proposal easier to understand. A small, isolated copy or value change may use text alone. Every core product, system, technical, or game-mechanic section must contain explanatory text plus at least one requirement-specific `architecture` or `flowchart` SVG. When the user explicitly requests visual design, every relevant section must contain its own visual block; when screens or interaction layout are part of that requirement, include a `ui-svg`. A visual stored under another requirement does not satisfy this rule. Do not add decorative diagrams that carry no design information.

Capture consequential choices as decisions with rationale, considered alternatives, consequences, and requirement links. Cover current-state constraints, proposed boundaries, contracts or data changes, error behavior, compatibility, rollout or migration, observability, security when relevant, and a validation strategy proportional to risk.

Keep facts and speculation distinct. If a missing product choice materially changes the design, preserve it as an open question rather than choosing invisibly. Do not add references to another plugin or external diagram workspace.

Before persisting, audit the result: confirm the project technical baseline makes concrete technology choices, the overall architecture exists, every requirement maps to exactly one section, every section contains a real design document, and every requested visual is present. Read [the workspace schema](../solution-studio/references/workspace-schema.md), validate each SVG block as standalone SVG, then call `solution_upsert_design` with the complete design document. After persisting, read the workspace back and verify the expected block IDs and types are present; do not treat a successful tool call alone as proof that the design is complete.
