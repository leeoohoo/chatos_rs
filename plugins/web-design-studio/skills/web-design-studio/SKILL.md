---
name: web-design-studio
description: Route editable website design work to project, component-system, responsive-layout, visual-system, and validation or export specialists in Web Design Studio.
metadata:
  chatos.role: router
  chatos.related-skills: "web-design-projects,web-design-components,web-design-responsive-layout,web-design-visual-system,web-design-validation-export"
---

# Web Design Studio

Use Web Design Studio when the user wants an editable website, landing page, interface composition, or focused visual revision. The primary operator is AI. The primary output is a coherent human-editable design that AI can generate, inspect, and revise through stable semantic IDs; code export is secondary and happens only when requested. Humans usually make small visual adjustments or attach annotations and component requests. Treat those notes as the next AI work queue, preserve their manual edits, and resolve each request only after the corresponding change exists.

## Runtime scope

ChatOS injects the outer user/project/public scope. Never ask for, invent, or pass a ChatOS project ID. Tool `projectId` values are internal Web Design Studio project identifiers returned by the plugin.

## Route the work

- Activate `web-design-projects` for project/document discovery, reuse, revisions, pages, annotations, and request queues.
- Activate `web-design-components` before choosing library bindings, slots, nesting, symbols, interactions, or reusable components.
- Activate `web-design-responsive-layout` for frames, constraints, Flex/Grid, auto layout, desktop/tablet/mobile behavior, and overflow repair.
- Activate `web-design-visual-system` for tokens, typography, color, spacing, imagery, effects, states, and coherent art direction.
- Activate `web-design-validation-export` before completion, conflict recovery, validation, preview, or HTML/React/Vue export.

Activate the specialist Skills that materially govern the requested edit. ChatOS associates them with this router internally. A substantial new page normally needs projects, components, responsive layout, visual system, and validation. A focused text edit may need only projects and validation.

## Platform Skill protocol

Activate this router through `skill_skill_activate`, then activate every leaf that materially governs the requested edit. ChatOS records the activation graph and validates business tools automatically. Call Web Design Studio tools with business arguments only; never add ChatOS user, project, workspace, session, activation, or authentication fields. Use the platform resource tools only for references declared by an activated Skill.

## AI-first operating loop

For a new website:

1. Discover existing projects and documents so retries revise the intended design instead of creating duplicates.
2. Call `web_design_get_document_outline` before reading nodes. Decide which single page to work on, then call `web_design_get_page` for that page only. On a large existing page, call `web_design_get_node` for the affected Frame/component instead of repeatedly reading the page.
3. Decompose the page into Figma-like semantic regions: for example App shell, Header, Sidebar, Hero, Feature grid, Pricing section, Form, and Footer. Do not treat the entire page as one region.
4. Read the compact catalog, search only the component families needed for the current region, and fetch full contracts only for selected production components.
5. Build one logical region at a time with `web_design_apply_node_batch`. Keep every visual object as its own editable node. A batch may contain closely related nodes, but it must never encode a complete page in one text node or one oversized payload.
6. Reread the page and run `web_design_validate` in `draft` mode after each meaningful region. Finish one page before moving to the next.
7. After all pages are complete, run `web_design_validate` in `handoff` mode. Do not report completion while blocking structure or quality issues remain.

For a human-adjusted website:

1. Read the document outline, then only the affected page and pending requests.
2. Treat current frames, styles, component IDs, annotations, and manual edits as authoritative state.
3. Apply the smallest revision-checked patch that satisfies the user's note.
4. Resolve only the request actually completed, then validate again.

Never require the human to translate a visual request into component IDs or protocol fields. Inspect the document and pending request records yourself.

## MCP tool directory

- Projects and documents: `web_design_list_projects`, `web_design_create_project`, `web_design_get_project`, `web_design_update_project`, `web_design_delete_project`, `web_design_list_documents`, `web_design_create_document`, and `web_design_move_document` manage the plugin's editable project structure inside the injected outer scope.
- Progressive reads: `web_design_get_document_outline` is the required first read for sparse page/root-layer metadata; `web_design_get_page` reads one page tree; `web_design_get_node` reads one bounded component subtree and ancestor path. `web_design_get_document` returns the complete document and is reserved for recovery or genuinely global work.
- Components and templates: `web_design_get_catalog` gives a compact overview; `web_design_search_components` returns bounded candidates; `web_design_get_component_contract` returns one exact binding contract; `web_design_insert_section` adds a bounded section; `web_design_apply_page_template` replaces one page from a template; `web_design_replace_document` replaces a complete editable document when a full redesign is intended.
- Focused editing: `web_design_apply_node_batch` constructs at most one logical page region with up to 24 independent nodes; `web_design_apply_patch` applies at most 48 small revision-checked operations on one page; `web_design_auto_layout` computes bounded layout changes; `web_design_sync_symbol_instances` refreshes instances from definitions; `web_design_update_symbol_from_instance` promotes an instance change into its reusable symbol.
- Requests: `web_design_list_requests` reads user-authored visual requests; `web_design_resolve_request` marks one request resolved only after the corresponding edit exists.
- Quality and delivery: `web_design_validate` checks document invariants and device layouts; `web_design_export_html`, `web_design_export_react`, and `web_design_export_vue` produce secondary code artifacts after the editable design is ready.

## Invariants

- Read outline first and the affected page second. Avoid whole-document reads during ordinary generation.
- Preserve stable IDs and unrelated user-authored content.
- Use focused patches for focused requests.
- Every visible object is an independent editable node. Text content represents one semantic text item only.
- Never use spaces, tabs, ASCII art, Markdown tables, or many newline-separated labels inside a text node to simulate navigation, cards, tables, forms, buttons, or a complete screen.
- Never send an entire multi-page design in one mutation. Work page by page and region by region.
- If a mutation is rejected, truncated, or too large, reread the current page and split the same intended structure into smaller batches. Never degrade to a text mockup, image mockup, or flattened replacement.
- Repeated UI belongs in library components or reusable Symbol/Instance structures. Use semantic layer names so AI and people can locate nodes later.
- Use container hierarchy and Flex/Grid layout to preserve design intent, like Figma Frame and Auto Layout, rather than relying only on unrelated absolute coordinates.
- Treat templates and sections as editable starting material, not a design boundary.
- Use `draft` validation while building and mandatory `handoff` validation before completion or export.
