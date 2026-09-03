---
name: web-design-studio
description: Design polished editable websites with Web Design Studio using Ant Design components, visual themes, responsive layouts, annotations, and page- or component-level AI design requests.
---

# Web Design Studio workflow

Use Web Design Studio when the user wants an editable website, landing page, UI composition, or focused visual changes in the design workbench. The primary outcome is a good-looking, coherent, human-editable design. Code export is secondary and should happen only when requested.

## Rules

1. List documents before creating a new one when the user may already have an active design.
2. Read the complete document and current revision before modifying it.
3. Call `web_design_get_component_library` before creating a page or introducing a new component family. Use its real Ant Design definitions, variants, sample data, and visual themes instead of inventing unsupported library bindings.
4. Preserve stable component IDs and use `web_design_apply_patch` for focused edits.
5. Never replace unrelated components when a request targets one selected component.
6. Treat open annotations and pending page- or component-level design requests as user requirements.
7. When a revision conflict occurs, read the document again and rebase the intended patch.
8. Keep components inside the viewport unless the user explicitly asks for overflow.
9. Use clear semantic component names such as `hero-heading`, `pricing-card-pro`, or `signup-email-input`.
10. Resolve a design request only after its requested visual change has been applied.
11. Run `web_design_validate` before reporting that a multi-component design task is complete.
12. When a request targets a responsive breakpoint, pass `device` on move, resize, and update operations instead of changing the desktop frame.
13. Preserve valid `parentId` relationships. Use `set_parent` for structural edits and avoid parent cycles.
14. For Flex/Grid containers, set the container layout first and then call `web_design_auto_layout` for each affected device.
15. Inspect `pages` and component `pageId` values before editing a multi-page document; keep components and their parents on the same page.
16. Establish or preserve a coherent visual system before polishing individual components. Use `set_tokens` for the global palette, typography, and radii. Prefer a curated theme returned by `web_design_get_component_library` when it fits the brief.
17. Product UI should use real Ant Design library bindings whenever a matching mature component exists. Keep native components for basic geometry and structural containers only.
18. An Ant Design component uses `library: { name: "antd", version, component, variant, props }`. Copy supported values from `web_design_get_component_library`, and keep structured example data editable in `props`.
19. Content-bearing Ant Design components expose `editableSlots`. Place editable child components inside one of those regions with both `parentId: <container id>` and `slot: <editable slot id>`. This applies to overlays, cards, forms, tabs, collapse panels, layouts, splitters, carousels, and other returned slot-capable components.
20. Build complete pages from semantic sections. Common production structures include navigation, hero, social proof, feature grids, product previews, pricing, FAQ, calls to action, contact forms, and footers.
21. Default to strong design craft: consistent spacing rhythm, restrained color use, clear type hierarchy, aligned edges, sufficient contrast, purposeful imagery, and no arbitrary decoration.
22. For a new or substantially redesigned page, check desktop, tablet, and mobile layouts before completion. Do not merely shrink the desktop composition.
23. Treat reusable definitions in `symbols` as shared assets. Preserve symbol and instance IDs unless intentionally detaching them.
24. Respect each symbol layer's `symbolOverrides` values: `content`, `style`, and `frame`.
25. Use `interaction` with `{ type: "page", target: pageId }` or an HTTPS URL for preview click behavior.
26. Export HTML, React, or Vue only when the user explicitly asks for a code deliverable.

## Typical workflow

1. Call `web_design_list_documents`.
2. Reuse the intended document or call `web_design_create_document`.
3. Call `web_design_get_document` and inspect pages, components, annotations, requests, tokens, and revision.
4. Call `web_design_get_component_library` when adding components or choosing a visual direction.
5. Apply focused operations with `web_design_apply_patch`.
6. If working from the request queue, call `web_design_resolve_request` after the requested design change succeeds.
7. Call `web_design_validate` and fix layout or structural issues before finishing.
8. Only when code delivery is requested, call the matching export tool and return the generated files.

## Component editing guidance

- Prefer `update_component` for content, frame-independent visual style, visibility, and interaction changes.
- Use `upsert_component` when adding or changing an Ant Design library binding because it preserves the complete typed component record.
- Use `move_component` and `resize_component` for layout changes.
- Use `set_breakpoint` to change desktop, tablet, or mobile canvas dimensions.
- Use `set_parent` to nest components and include `slot` when the parent exposes editable Ant Design content regions. Use `set_layout` to configure `free`, `flex-row`, `flex-column`, or `grid` containers.
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
