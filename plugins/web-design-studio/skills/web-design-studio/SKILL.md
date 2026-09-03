---
name: web-design-studio
description: Design polished editable websites with Web Design Studio using independently grouped Ant Design, Chakra UI, and shadcn/ui components, visual themes, responsive layouts, annotations, and page- or component-level AI design requests.
---

# Web Design Studio workflow

Use Web Design Studio when the user wants an editable website, landing page, UI composition, or focused visual changes in the design workbench. The primary outcome is a good-looking, coherent, human-editable design. Code export is secondary and should happen only when requested.

## Rules

1. Treat the ChatOS runtime scope as authoritative. The outer ChatOS project ID is injected by the host as `CHATOS_PROJECT_ID` and is returned as `scope.chatosProjectId`; never invent it, copy it into tool arguments, or ask the user to provide it.
2. Keep the two project identities separate. A tool argument named `projectId` always means an internal Web Design Studio project returned by `web_design_list_projects` or `web_design_get_project`; it never means the outer ChatOS project ID.
3. Start project-aware work with `web_design_list_projects`. Reuse the intended internal project or create one with `web_design_create_project`, then list or create designs inside it. A website project may contain multiple separately named designs, and each design may contain multiple pages.
4. List documents before creating a new one when the user may already have an active design.
5. Read the complete document and current revision before modifying it.
6. Call `web_design_get_component_library` before creating a page or introducing a new component family. Select one of its independently grouped Ant Design, Chakra UI, or shadcn/ui definitions and use the returned variants, sample data, slots, and visual themes instead of inventing unsupported bindings.
7. Preserve stable component IDs and use `web_design_apply_patch` for focused edits.
8. Never replace unrelated components when a request targets one selected component.
9. Treat open annotations and pending page- or component-level design requests as user requirements.
10. When a revision conflict occurs, read the document again and rebase the intended patch.
11. Keep components inside the viewport unless the user explicitly asks for overflow.
12. Use clear semantic component names such as `hero-heading`, `pricing-card-pro`, or `signup-email-input`.
13. Resolve a design request only after its requested visual change has been applied.
14. Run `web_design_validate` before reporting that a multi-component design task is complete.
15. When a request targets a responsive breakpoint, pass `device` on move, resize, and update operations instead of changing the desktop frame.
16. Preserve valid `parentId` relationships. Use `set_parent` for structural edits and avoid parent cycles.
17. For Flex/Grid containers, set the container layout first and then call `web_design_auto_layout` for each affected device.
18. Inspect `pages` and component `pageId` values before editing a multi-page document; keep components and their parents on the same page.
19. Establish or preserve a coherent visual system before polishing individual components. Use `set_tokens` for the global palette, typography, and radii. Prefer a curated theme returned by `web_design_get_component_library` when it fits the brief.
20. Product UI should use a mature library binding whenever a matching component exists. Keep native components for basic geometry only. Do not silently mix design systems inside one section: preserve the user's chosen library unless a mixed-library composition is explicitly requested.
21. A library component uses `library: { name: "antd" | "chakra" | "shadcn", version, component, variant, props }`. Copy supported values from the matching library group returned by `web_design_get_component_library`, and keep structured example data editable in `props`.
22. Content-bearing components expose `editableSlots` through the same contract in all three libraries. Place editable child components inside one of those regions with both `parentId: <container id>` and `slot: <editable slot id>`. This applies to overlays, cards, fields, tabs, accordions, layouts, splitters, scroll areas, and other returned slot-capable components.
23. Build complete pages from semantic sections. Common production structures include navigation, hero, social proof, feature grids, product previews, pricing, FAQ, calls to action, contact forms, and footers.
24. Default to strong design craft: consistent spacing rhythm, restrained color use, clear type hierarchy, aligned edges, sufficient contrast, purposeful imagery, and no arbitrary decoration.
25. For a new or substantially redesigned page, check desktop, tablet, and mobile layouts before completion. Do not merely shrink the desktop composition.
26. Treat reusable definitions in `symbols` as shared assets. Preserve symbol and instance IDs unless intentionally detaching them.
27. Respect each symbol layer's `symbolOverrides` values: `content`, `style`, and `frame`.
28. Use `interaction` with `{ type: "page", target: pageId }` or an HTTPS URL for preview click behavior.
29. Export HTML, React, or Vue only when the user explicitly asks for a code deliverable.
30. Prefer a returned production page template or section preset as the starting structure for product sites, brand sites, portfolios, and campaign pages. Do not approximate a marketing site by stacking dashboard cards and form controls.
31. Treat full-page templates and reusable page sections as editable component trees, not opaque screenshots. Preserve their parent relationships and responsive frames when refining them.
32. Use `web_design_apply_page_template` for a new full product, brand, portfolio, campaign, or developer page when a matching template exists. Use `web_design_insert_section` to add a single narrative region without replacing the rest of the page.

## Typical workflow

1. Call `web_design_list_projects` and note the host-injected `scope.chatosProjectId` without passing it back as an argument.
2. Select or create an internal Web Design Studio project and use its returned `projectId` for subsequent project-scoped tools.
3. Call `web_design_list_documents` with that internal `projectId`.
4. Reuse the intended document or call `web_design_create_document` with that internal `projectId`.
5. Call `web_design_get_document` and inspect pages, components, annotations, requests, tokens, and revision.
6. Call `web_design_get_component_library` when adding components or choosing a visual direction; inspect its `sections` and `pageTemplates` before assembling a visually rich public-facing page.
7. Apply a suitable full-page starting point with `web_design_apply_page_template`, or add individual regions with `web_design_insert_section`.
8. Refine the result with focused operations through `web_design_apply_patch` instead of discarding the generated structure.
9. If working from the request queue, call `web_design_resolve_request` after the requested design change succeeds.
10. Call `web_design_validate` and fix layout or structural issues before finishing.
11. Only when code delivery is requested, call the matching export tool and return the generated files.

## Component editing guidance

- Prefer `update_component` for content, frame-independent visual style, visibility, and interaction changes.
- Use `upsert_component` when adding or changing any UI library binding because it preserves the complete typed component record.
- Use `move_component` and `resize_component` for layout changes.
- Use `set_breakpoint` to change desktop, tablet, or mobile canvas dimensions.
- Use `set_parent` to nest components and include `slot` when the parent exposes editable library content regions. Use `set_layout` to configure `free`, `flex-row`, `flex-column`, or `grid` containers.
- For Tabs and Collapse, use the exact per-panel slot IDs returned by `editableSlots`; do not put every child into a generic content slot.
- Use `web_design_auto_layout` after structural or container-layout changes.
- Assign `pageId` on newly created components and keep parent-child relationships on the same page.
- Reuse existing image assets when possible instead of embedding duplicates.
- Use `set_tokens` to update the global colors, radii, and typography without rewriting every component.
- Use `web_design_sync_symbol_instances` after changing a reusable definition.
- Use `web_design_update_symbol_from_instance` when a selected instance should become the shared definition.
- Pass `device: "tablet"` or `device: "mobile"` for breakpoint-specific layout and style changes.
- Use stable, descriptive IDs rather than random IDs when AI creates components.
- Preserve user-authored positions, sizes, styles, data, and annotations unless the request explicitly changes them.
