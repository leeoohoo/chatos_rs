---
name: web-design-studio
description: Primary entry point for creating or revising visually strong, editable website and interface artboards with Web Design Studio.
metadata:
  chatos.role: router
  chatos.related-skills: "web-design-planning,web-design-scene-building,web-design-candidate-review,web-design-documents,web-design-components,web-design-responsive-layout,web-design-visual-system,web-design-validation-export"
---

# Web Design Studio

Use this Skill as the entry point for website, landing-page, product-interface, menu, modal, Drawer, overlay, and important UI-state design. The primary deliverable is an editable visual Scene with stable semantic nodes. Code and interaction wiring are optional later phases, never substitutes for visual design.

## Hard delivery gate

Every planning and generation response includes `deliveryGate`. It is authoritative.

- If `visibleSceneReady` is false, keep designing in the plugin. Do not edit application UI source or report a result.
- If `projectImplementationAllowed` is false, no artboard has completed visual handoff. Do not implement it in product code.
- Implement only artboards counted by `completedArtboardCount`.
- Report the requested design scope complete only when `taskCompletionAllowed` is true.

A document, Site Plan, page plan, canonical root, successful mutation, or passing structural check is not visible delivery.

## One canvas, one semantic artboard

A project may contain many artboards, but they form a directory rather than one world-space composition. The editor and AI mount exactly one chosen artboard canvas at a time. The human may click a directory entry, and the AI may choose the relevant `artboardId` itself from `web_design_get_active_context.artboardDirectory`.

Artboards represent independently designed surfaces: a route, expanded menu, modal, Drawer, Popover, overlay, or meaningful state. Width and height are properties of an artboard. Do not create desktop, tablet, and mobile copies by default; responsive widths belong to the same semantic artboard unless the user requests independent variants.

When the brief does not specify a desktop width, use 1440 CSS px as the working artboard width. Treat 1200 as an explicitly chosen compact compatibility width, not the standard default. For products expected on 2K/QHD displays, keep the same semantic artboard and also validate its responsive behavior at 1920 and 2560 CSS px when those widths are relevant; do not confuse physical monitor pixels with the browser's CSS viewport.

Artboard height is content-driven. Use the selected height only as the initial minimum visible editing surface (900px for the default 1440 desktop artboard), and let the artboard grow immediately to the lowest visible node. Do not lock a page, inner full-page frame, or application shell to 768/900px merely because that is a familiar screen height. Fixed dimensions are appropriate only for a deliberately bounded surface such as a modal, Drawer, or device-specific state requested by the user.

For focused work, choose one directory entry and pass that `artboardId` to Scene query/edit tools. Generation Steps use the one page selected by the Plan. Never load several artboards merely to decide what to edit, and preserve every non-target artboard unchanged.

## Load only the stage you need

Activate leaf Skills just in time:

- `web-design-documents`: locate or create the scoped document and handle pending human requests.
- `web-design-planning`: define the semantic artboard inventory and plan one active artboard.
- `web-design-components`: search the actual component, section, template, and theme supply; inspect selected component contracts.
- `web-design-scene-building`: create or revise editable Scene nodes for one bounded Step.
- `web-design-visual-system`: make typography, color, imagery, spacing, surface, and art-direction decisions.
- `web-design-responsive-layout`: repair only the viewport widths required by the brief.
- `web-design-candidate-review`: inspect Candidate and Diff images, accept/reject, resume, or handle an annotation.
- `web-design-validation-export`: run final visual validation and optional export after handoff.

Do not activate every leaf at startup. Tool gates state which leaf is required.

## Normal design loop

1. Activate `web-design-documents` and `web-design-planning`, then call `web_design_get_active_context`. Read its compact `artboardDirectory`, choose the one artboard relevant to the task, and resume any active Step or returned `resumeReview`; do not create duplicates.
2. Build a short evidence map from the user's brief and in-scope project content: product, audience, promise, required content, existing visual cues, and unknowns. Runtime names, repository folder names, Skill prose, component demos, and test fixtures are not customer-brand evidence.
3. Use `web_design_plan_site` for the semantic artboard inventory. Then use `web_design_plan_page` for exactly one artboard. Planning the page automatically selects it, starts it, and creates its canonical Scene root.
4. Before construction, activate `web-design-components`. Read the small catalog summary, search only relevant kinds, and inspect the exact contract for every library component chosen. Catalog material informs the composition but never chooses the art direction.
5. Activate `web-design-scene-building` and the visual-system or responsive leaf needed by the current Step. Call `web_design_execute_step` with the editable node changes. The plugin automatically captures the current revision at every required viewport and decides whether this is an initial run, retry, or repair.
6. Prefer `insert-simple-tree` for a visible hierarchy. Every descendant remains an independent editable node. Use smaller operations for focused changes; never simulate a UI with whitespace, ASCII art, one giant text node, or a flattened screenshot.
7. Activate `web-design-candidate-review`. Inspect every returned Candidate and Diff image. Mechanical layout success is necessary but not sufficient. Use `web_design_control_plan` to accept or reject the reviewed Candidate explicitly.
8. Continue the returned next action. Accepting the handoff Step automatically completes that artboard and returns a compact context checkpoint. Move to the next semantic artboard only then, unless the user asks to pause.

## Visual quality bar

Accept only when the screenshot demonstrates the intended improvement:

- The focal point and reading order are obvious without reading layer names.
- Typography, contrast, spacing, alignment, density, and whitespace form a deliberate hierarchy.
- The composition has purposeful variation rather than equal cards or repeated demo rows.
- Color, imagery, surfaces, and detail fit the actual product and audience.
- The edited region belongs with its neighbors and remains legible at all required widths.
- Nothing is accidentally empty, clipped, overlapping, overflowing, or generic.

Public-facing and expressive sites should not default to sidebars, KPI tiles, dense tables, settings panels, or dashboard grids. Use those patterns only when the brief is genuinely administrative, analytical, operational, or data-heavy.

## Non-negotiable invariants

- Inspect real PNG content; never infer visual quality from JSON or successful tool status.
- Preserve stable IDs, manual changes, locked fields, inactive artboards, and unrelated content.
- One execution call prepares one bounded Step Candidate. Several sequential Steps may run in one user task.
- Candidate generation and acceptance remain separate; visual review is never automated away.
- Use semantic node names. One text node contains one semantic text item.
- Use Frame, Auto Layout, Grid, and responsive overrides to encode design intent.
- Do not use component libraries or templates as whole-page creative direction.
- Never fabricate artifact IDs or reuse evidence from a stale Scene revision.
- If a mutation is too large, use `insert-simple-tree` or split by semantic region. Do not reduce editability or visual content.
- Do not perform dependency upgrades, audit fixes, lockfile rewrites, or unrelated source changes during design work.
- For design plus implementation, complete the editable Scene artboard first, then implement only that accepted design.
