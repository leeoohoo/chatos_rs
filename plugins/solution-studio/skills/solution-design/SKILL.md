---
name: solution-design
description: Create or revise a traceable product and technical design from structured requirements, including text, code-generated architecture diagrams, flowcharts, and SVG interface designs. Use before implementation planning.
---

# Solution Design

Design against a concrete requirements revision. Read the current workspace first and set `basedOnRequirementsRevision` to the revision actually used. Produce one decided solution for the project, not a menu of implementation paths.

## Reliable authoring sequence

1. Before writing, derive a delivery inventory from the request. Give every required text, architecture, flowchart, and interface visual its final stable block ID and type. If the user asks for two screens, the inventory contains two distinct `ui-svg` blocks.
2. Call `solution_upsert_design` with the decided project baseline, sections, text documents, decisions, and any visuals that fit comfortably in the complete payload. It is valid for this first saved revision to be structurally incomplete while the remaining named visual blocks are being added; do not present it as delivered.
3. Add or correct substantial visuals one at a time with `solution_upsert_design_block`, always using inline SVG in `block.content` and the latest returned workspace revision. This keeps one invalid SVG from discarding unrelated valid design content.
4. If a tool reports a missing Skill activation, activate the named Solution Studio Skill and retry the same business operation. If it reports invalid SVG, repair the inline SVG according to the exact validation error and retry the same block ID. Never switch to writing a project file as a fallback.
5. After all blocks are saved, call `solution_get_workspace`; compare the saved IDs and types against the delivery inventory, then render and inspect each saved visual.
6. If design changed after an execution plan existed, revise the execution plan against the new design revision. Finally call `solution_finalize` with the complete visual inventory.

Use `design.blocks` for the project-level solution. For a development project it must contain a technical-baseline text block and an overall architecture SVG. The technical baseline must explicitly choose the language and runtime, application or game framework, UI/rendering technology, state and rules architecture, data storage or database, testing toolchain, build system, and deployment shape. State when a server database is intentionally not needed. The overall architecture must show the major runtime boundaries and data flow.

Create exactly one `DesignSection` for each requirement and set that requirement's `selectedDesignSectionId` to it. A section is the requirement's detailed design inside the fixed project architecture, not a candidate. Record rejected or considered alternatives only in `decisions[].alternatives`; never create a second design section to represent another route.

For a development project, every section needs a substantive text block. A one-sentence `body` is only the summary and never counts as the design document. Cover the relevant module responsibilities, interfaces or data model, state and key flow, error and fallback behavior, compatibility or rollout constraints, observability, and verification. Omit only headings that genuinely do not apply, not the document itself.

Read [design content types](references/design-content-types.md) before generating or revising blocks. Visual blocks contain self-contained SVG source code produced directly for this workspace. Do not call, reference, bind to, or create assets in another diagram or design plugin. Do not save planning Markdown or SVG files under the host project root. The code belongs in `DesignContentBlock.content`; Solution Studio stores it in isolated plugin data and renders it locally.

Use only the block types that make the proposal easier to understand. A small, isolated copy or value change may use text alone. Every core product, system, technical, or game-mechanic section must contain explanatory text plus at least one requirement-specific `architecture` or `flowchart` SVG. When the user explicitly requests visual design, every relevant section must contain its own visual block; when screens or interaction layout are part of that requirement, include a `ui-svg`. A visual stored under another requirement does not satisfy this rule. Do not add decorative diagrams that carry no design information.

Capture consequential choices as decisions with rationale, considered alternatives, consequences, and requirement links. Cover current-state constraints, proposed boundaries, contracts or data changes, error behavior, compatibility, rollout or migration, observability, security when relevant, and a validation strategy proportional to risk.

Keep facts and speculation distinct. If a missing product choice materially changes the design, preserve it as an open question rather than choosing invisibly. Do not add references to another plugin or external diagram workspace.

Before persisting, audit the result: confirm the project technical baseline makes concrete technology choices, the overall architecture exists, every requirement maps to exactly one section, every section contains a real design document, and every requested visual is present. Read [the workspace schema](../solution-studio/references/workspace-schema.md), validate each SVG block as standalone SVG, then call `solution_upsert_design` with the complete design document. For a large SVG or a correction to one existing block, call `solution_upsert_design_block` with inline content and optimistic `expectedRevision`; never fall back to a project-directory file when a complete-document upsert fails.

After the final write, read the workspace back and compare the expected inventory with the saved block IDs, types, and non-empty contents. Open or render every saved SVG preview and confirm that visible shapes and labels appear. Then call `solution_finalize` with `scope: "design"` (or `"full"` for an end-to-end plan), the current workspace revision, and `expectedDesignBlocks` containing every visual promised in the request. An `<svg>` wrapper, an external `.svg` file, a successful upsert, or source code that was never rendered does not satisfy delivery.
