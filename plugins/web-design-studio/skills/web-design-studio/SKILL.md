---
name: web-design-studio
description: Route AI-first, visual-first, progressively generated website design work to Web Design Studio specialists while keeping interaction and code secondary.
metadata:
  chatos.role: router
  chatos.related-skills: "web-design-progressive-generation,web-design-documents,web-design-components,web-design-responsive-layout,web-design-visual-system,web-design-validation-export"
---

# Web Design Studio

Use Web Design Studio when the user wants an editable website, landing page, interface composition, or focused visual revision. The primary operator is AI. The primary output is a visually strong, coherent, human-editable design that AI can generate, inspect, and revise through stable semantic IDs. Interaction behavior and code export are secondary and happen only when required. Humans usually review screenshots, make small visual adjustments, or attach annotations and component requests. Treat those notes as the next AI work queue, preserve their manual edits, and resolve each request only after the corresponding visible change exists.

## Runtime scope

ChatOS binds the active document scope before the plugin starts. Use the document tools exactly as declared; all documents returned by them already belong to the current execution scope.

For every AI generation or substantial AI redesign, activate `web-design-progressive-generation` first. Its Plan, Page, Step, Candidate, visual-evidence, and page-boundary workflow is the only generation path. Direct node tools may implement only the current bounded Step or a focused repair; they are never an alternative whole-page generator.

## Route the work

- Activate `web-design-documents` for document discovery, reuse, Scene pages, annotations, and request queues.
- Activate `web-design-progressive-generation` for site/page planning, resumable AI generation, visual Candidate review, retry, pause, and recovery.
- Activate `web-design-components` before choosing library bindings, slots, nesting, symbols, interactions, or reusable components.
- Activate `web-design-responsive-layout` for frames, constraints, Flex/Grid, auto layout, desktop/tablet/mobile behavior, and overflow repair.
- Activate `web-design-visual-system` for tokens, typography, color, spacing, imagery, effects, states, and coherent art direction.
- Activate `web-design-validation-export` before completion, conflict recovery, validation, preview, or HTML/React/Vue export.

Activate the specialist Skills that materially govern the requested edit. ChatOS associates them with this router internally. A substantial new page normally needs progressive generation, documents, components, visual system, validation, and responsive layout only when the brief requires responsive behavior. A focused text edit may need only documents and validation.

## Platform Skill protocol

Activate this router through `skill_skill_activate`, then activate every leaf that materially governs the requested edit. ChatOS records the activation graph and validates business tools automatically. Call Web Design Studio tools only with their declared business arguments. Execution identity, scope, session, activation, and authentication context are supplied by the runtime. Use the platform resource tools only for references declared by an activated Skill.

## AI-first operating loop

For a new website or substantial redesign:

1. Read active context and list documents so a retry resumes the intended design instead of creating a duplicate.
2. Plan the site inventory without generating it. A destination page, menu destination, modal, Drawer, overlay, or important interface state is a separate artboard that can be designed independently and linked later.
3. Select exactly one artboard and save its audience, purpose, art direction, focal hierarchy, composition, typography, color, imagery strategy, content hierarchy, and screenshot-based acceptance criteria.
4. Start or resume that artboard. Execute only its current `nextAction` as one bounded visual Step. An artboard may and usually should remain incomplete across several calls or sessions.
5. Decompose work into small passes such as composition skeleton, primary focal region, one supporting section, typography and color application, imagery treatment, responsive visual repair, and final polish. Do not interpret “work on one page” as “complete one page now.”
6. Before and after every Step, inspect real rendered images and their grounding. Use direct node and component tools only inside that Step's target subtree.
7. Run draft structural validation after the Step, but judge acceptance from the screenshot: focal point, hierarchy, rhythm, whitespace, brand relevance, legibility, and absence of accidental template or dashboard character.
8. Commit, retry, reject, or pause only that Step. Continue only through a later invocation or explicit next action. Run the visual Design Gate and handoff validation only when the artboard is genuinely ready.

Do not default a public-facing or expressive website to a sidebar, KPI cards, tables, settings panels, or a grid of interchangeable cards. Do not spend generation Steps proving that controls click. Unless the brief is specifically for an admin or data product, prioritize editorial composition, brand character, typography, imagery, visual storytelling, and deliberate whitespace.

For a human-adjusted website:

1. Read the active context and pending requests. A `scene-annotation` request is authoritative and already includes its page, stable node, Scene revision, and `web_design_prepare_annotation_task` arguments.
2. Prepare that Scene annotation before editing. Inspect the returned PNG crop and grounding; do not infer the requested visual change from node metadata alone.
3. Treat current frames, styles, stable IDs, annotations, and manual edits as authoritative state. Apply the smallest revision-checked Scene edit inside the task scope.
4. Capture the changed region and compare it with the prepared snapshot. Resolve the annotation only after the actual image shows the requested design result and validation passes.

Never require the human to translate a visual request into component IDs or protocol fields. Inspect the document and pending request records yourself.

## MCP tool directory

- Documents: `web_design_list_documents` lists designs in the active scope. `web_design_create_document` creates a design in that same scope. List first and reuse a matching document before creating another one.
- Progressive state: `web_design_get_active_context`, `web_design_get_plan`, and `web_design_inspect_step` are the authoritative resume and review reads. Use `web_design_query_scene` for bounded page, role, type, name, or stable-ID reads from the current Scene revision.
- Visual evidence: `web_design_capture_page` and `web_design_capture_region` return real Chromium PNGs; `web_design_get_visual_grounding` maps pixels to stable Scene IDs; `web_design_inspect_at_point` resolves ambiguous image coordinates; `web_design_compare_snapshots` verifies the actual before/after impact. Use these for every progressive visual decision instead of judging from node metadata alone.
- Components: `web_design_get_catalog` gives a compact library overview; `web_design_search_components` returns bounded candidates; `web_design_get_component_contract` returns the official library binding and Scene binding fields for the one chosen component. Insert it only as a `library-instance` node inside the current Step Candidate.
- Focused editing: AI construction happens through the `operations` of `web_design_run_next_step`, `web_design_retry_step`, or `web_design_repair_step`. `web_design_edit_scene` is only for a small, already-scoped adjustment after a Scene read; it is not a generation bypass.
- Requests: `web_design_list_requests` returns Scene annotations. Use `web_design_prepare_annotation_task` to bind one open annotation to its exact page, stable node, Scene revision, PNG crop, and grounding before editing.
- Review and recovery: `web_design_accept_step`, `web_design_reject_step`, `web_design_rollback_step`, `web_design_pause_plan`, and `web_design_resume_plan` change only the current Plan/Step. Code export is not part of the design-completion path.

The old document `components[]` read, patch, template, auto-layout, symbol, request-resolution, and export tools are not an alternative API. Do not call them for new generation, revision, recovery, or handoff. Scene v2 is the only editable design truth.

## Invariants

- Read active context and Plan first, then query only the affected Scene page or subtree. Avoid complete legacy-document reads.
- The injected scope is immutable. “Second version”, “redesign”, and alternative visual directions are document or page decisions inside the active scope.
- Preserve stable IDs and unrelated user-authored content.
- Never fabricate visual artifact IDs or claim visual completion without inspecting the returned PNG content.
- Use one bounded Scene Candidate for one focused request.
- Every visible object is an independent editable node. Text content represents one semantic text item only.
- Never use spaces, tabs, ASCII art, Markdown tables, or many newline-separated labels inside a text node to simulate navigation, cards, tables, forms, buttons, or a complete screen.
- Never send an entire multi-page design in one mutation. Work page by page and region by region.
- Never attempt to finish even one complex page in one invocation. One invocation advances at most one bounded Step.
- Never treat functional interaction, component count, or structural validation as proof of visual quality.
- Do not let a component library or template determine the art direction. Components are editable visual materials inside a page-specific composition.
- Do not default to admin-dashboard composition unless the user explicitly asks for an admin, operations, analytics, or data-management interface.
- If a mutation is rejected, truncated, or too large, reread the current page and split the same intended structure into smaller batches. Never degrade to a text mockup, image mockup, or flattened replacement.
- Repeated UI belongs in library instances or Scene component-main/component-instance structures. Use semantic layer names so AI and people can locate nodes later.
- Use container hierarchy and Flex/Grid layout to preserve design intent, like Figma Frame and Auto Layout, rather than relying only on unrelated absolute coordinates.
- Treat templates and sections as editable starting material, not a design boundary.
- Use `draft` validation while building and mandatory `handoff` validation before completion or export.
