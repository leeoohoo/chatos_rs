---
name: web-design-studio
description: Create and edit structured website designs with Web Design Studio, including component-level annotations and pending AI design requests.
---

# Web Design Studio workflow

Use Web Design Studio when the user wants an editable website mockup, landing page, UI composition, or focused changes to a design already open in the visual workbench.

## Rules

1. List documents before creating a new one when the user may already have an active design.
2. Read the complete document and current revision before modifying it.
3. Preserve stable component IDs and use `web_design_apply_patch` for focused edits.
4. Never replace unrelated components when a request targets one selected component.
5. Treat open component annotations and pending design requests as user requirements.
6. When a revision conflict occurs, read the document again and rebase the intended patch.
7. Keep components inside the viewport unless the user explicitly asks for overflow.
8. Use clear semantic component names such as `hero-heading`, `pricing-card-pro`, or `signup-email-input`.
9. Resolve a design request only after its requested visual change has been applied.
10. Run `web_design_validate` before reporting that a multi-component design task is complete.
11. When a request targets a responsive breakpoint, pass `device` on move, resize, and update operations instead of changing the desktop frame.
12. Preserve valid `parentId` relationships. Use `set_parent` for structural edits and avoid parent cycles.
13. For Flex/Grid containers, set the container layout first and then call `web_design_auto_layout` for each affected device.
14. Inspect `pages` and component `pageId` values before editing a multi-page document; keep components and their parents on the same page.
15. Use `web_design_export_html` when the user asks for an HTML deliverable, and specify the intended page and responsive device when known.
16. Preserve shared design tokens when editing styles. Prefer token references such as `var(--color-primary)` when the design already uses them.
17. Treat reusable component definitions in `symbols` as shared assets. Preserve `symbolId`, `symbolInstanceId`, and `symbolComponentId` on instances unless intentionally detaching them.
18. Use `web_design_sync_symbol_instances` after changing a definition. Respect each layer's `symbolOverrides` values: `content`, `style`, and `frame`.
19. Use `web_design_update_symbol_from_instance` when the user wants the selected instance to become the shared definition; it also synchronizes other instances.
20. Use `interaction` with `{ type: "page", target: pageId }` or an HTTPS URL interaction for preview/export click behavior.
21. Use `web_design_export_react` for React and `web_design_export_vue` for Vue; both export all pages with client-side route navigation.
22. Available component types are `section`, `card`, `divider`, `heading`, `text`, `button`, `link`, `image`, `video`, `icon`, `logo`, `input`, `textarea`, `select`, `checkbox`, `switch`, `badge`, `avatar`, `list`, and `table`.
23. Build complete pages from semantic sections instead of using only generic cards. Common production structures include navigation, hero, feature grids, pricing, FAQ, contact forms, and footers.

## Typical workflow

1. Call `web_design_list_documents`.
2. Reuse the intended document or call `web_design_create_document`.
3. Call `web_design_get_document` and inspect component IDs, annotations, requests, and revision.
4. Apply one or more focused operations with `web_design_apply_patch`.
5. If working from the request queue, call `web_design_resolve_request` after the patch succeeds.
6. Call `web_design_validate` and mention any remaining open annotations or pending requests.
7. When delivery is requested, call `web_design_export_html` for one page or all pages and return the generated filenames.
8. For React or Vue delivery, call the matching export tool and return `WebDesignApp.jsx` or `WebDesignApp.vue`.

## Component editing guidance

- Prefer `update_component` for content and visual style changes.
- Use `move_component` and `resize_component` for layout changes.
- Use `set_breakpoint` to change desktop, tablet, or mobile canvas dimensions.
- Use `set_parent` to nest components and `set_layout` to configure `free`, `flex-row`, `flex-column`, or `grid` containers.
- Use `web_design_auto_layout` after structural or container-layout changes; it applies the configured gap, padding, columns, and alignment to direct children.
- Use `upsert_page` and `remove_page` to manage pages. Page slugs must begin with `/` and remain unique.
- Assign `pageId` on newly created components; preserve the current page when making focused edits.
- Use `upsert_asset` and `remove_asset` for base64 image resources. Prefer reusing an existing asset data URL instead of adding duplicates.
- `web_design_export_html` can export the desktop, tablet, or mobile layout as standalone files.
- Use `set_tokens` to update global color, radius, and typography values without rewriting unrelated components.
- Use `upsert_symbol` and `remove_symbol` to manage reusable component definitions.
- Components originating from a reusable definition carry symbol, instance, and source-layer IDs. Set `symbolOverrides` only for properties that must remain local to that instance layer.
- Use `web_design_sync_symbol_instances` for definition-to-instance propagation and `web_design_update_symbol_from_instance` for instance-to-definition propagation.
- `web_design_export_react` exports every page and uses page slugs for browser history routing.
- `web_design_export_vue` exports the same routed design as a Vue single-file component.
- Pass `device: "tablet"` or `device: "mobile"` for breakpoint-specific layout and style changes.
- Use `upsert_component` for new components with complete valid fields.
- Use stable, descriptive IDs instead of random IDs when AI creates components.
- Choose the most specific component type so HTML, React, and Vue exports retain useful form and data-display semantics.
- Preserve user-authored positions, sizes, style fields, and annotations unless the request explicitly changes them.
