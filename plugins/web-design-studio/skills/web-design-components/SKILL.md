---
name: web-design-components
description: Build editable, visually purposeful Web Design Studio component trees with supported libraries, slots, hierarchy, symbols, bounded interactions, and stable semantic IDs.
metadata:
  chatos.role: leaf
---

# Component systems

## Figma-like node model

Think in the Scene v2 Page → semantic Frame/container → child node tree:

- A page is not a component and must not be represented by one giant text box.
- A logical region such as Header, Sidebar, Hero, Product grid, Checkout form, or Footer is a `section`, `card`, or supported library content container.
- Every title, label, paragraph, icon, image, field, button, badge, divider, table, and card is an independent node with a stable semantic ID and name.
- Repeated structures use `library-instance` or Scene component-main/component-instance nodes. Preserve explicit overrides rather than copying loosely related shapes.
- Containers express layout intent through Flex/Grid, gap, padding, alignment, wrapping, and responsive constraints. Absolute coordinates are still valid for intentional free composition, not as a substitute for structure.

Before construction, read active context and the current Plan, choose its single active page, then call `web_design_query_scene` for that page or target node IDs. Break the page into logical visual regions. Submit one bounded set of Scene operations through the current progressive Step. A large region must become several Steps rather than one large batch.

Use the catalog progressively instead of loading every component into model context:

1. Call `web_design_get_catalog` once to choose a coherent design system, theme, template, or production section.
2. Call `web_design_search_components` with the user's intent and an explicit limit to find a small candidate set.
3. Call `web_design_get_component_contract` only for the selected component.
4. Copy the returned Scene binding fields—library, component, variant, properties, content, default size, and slots—exactly into one `library-instance` node. Never invent a binding.

Choose components only after the current Step has a visual intention. A library is a source of reliable primitives and variants, not a page template or art director. Select the variant whose shape, density, emphasis, and state support the composition; do not insert several near-identical variants merely to demonstrate the catalog.

- Use Ant Design, Chakra UI, or shadcn/ui for mature product controls when a matching component exists.
- Use native geometry and typography for bespoke composition, backgrounds, decoration, and simple content.
- Do not silently mix design systems inside one section.
- Place editable children inside declared Scene slots using both `parentId` and `slot` in Candidate operations.
- Preserve page-local parent relationships and prevent cycles.
- Use semantic stable IDs such as `hero-heading` and `pricing-card-pro`.
- Treat component-main/component-instance nodes as shared Scene definitions. Preserve stable main and instance IDs and explicit overrides.
- Use interactions for page navigation or HTTPS links only when the brief requires behavior and the visual Design Gate has passed. Represent important destinations, modal content, Drawers, menus, and interface states as separate editable artboards where practical.
- Prefer a small number of purposeful components over filling the canvas with every available control.
- Unless the brief explicitly requires an admin or data product, do not default to dashboard shells, metric cards, tables, filters, or settings forms just because mature libraries provide them.
- Vary composition through hierarchy, scale, spacing, imagery, and custom geometry instead of manufacturing variety with interchangeable component variants.
- A text node contains exactly one semantic text value. Do not format several controls or rows with spaces, tabs, pipes, indentation, Markdown, or newlines.
- A tool-size or parsing failure means the batch is too large. Reread the page, reduce the region, and retry with the same independent-node structure. Never replace the intended interface with descriptive prose.

Do not use the old `components[]`, node-batch, patch, template, Symbol, or export APIs. They are not a compatibility path. The Candidate transaction must produce editable Scene nodes. Read [component examples](references/examples.md) before mixing library and custom composition.
