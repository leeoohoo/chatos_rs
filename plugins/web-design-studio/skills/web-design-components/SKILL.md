---
name: web-design-components
description: Build editable Web Design Studio component trees with supported libraries, slots, hierarchy, symbols, interactions, and stable semantic IDs.
metadata:
  chatos.role: leaf
---

# Component systems

## Figma-like node model

Think in a Page → semantic Frame/container → child node tree:

- A page is not a component and must not be represented by one giant text box.
- A logical region such as Header, Sidebar, Hero, Product grid, Checkout form, or Footer is a `section`, `card`, or supported library content container.
- Every title, label, paragraph, icon, image, field, button, badge, divider, table, and card is an independent node with a stable semantic ID and name.
- Repeated structures use a library component or Symbol/Instance. Preserve explicit overrides rather than copying loosely related shapes.
- Containers express layout intent through Flex/Grid, gap, padding, alignment, wrapping, and responsive constraints. Absolute coordinates are still valid for intentional free composition, not as a substitute for structure.

Before construction, call `web_design_get_document_outline`, choose one page, and call `web_design_get_page`. For focused work inside an existing region, call `web_design_get_node` on that semantic container. Break the page into logical regions. Use one `web_design_apply_node_batch` per region, normally 4–16 nodes and never more than 24. A large region should be split into subregions such as `Header / brand`, `Header / navigation`, and `Header / actions`.

Use the catalog progressively instead of loading every component into model context:

1. Call `web_design_get_catalog` once to choose a coherent design system, theme, template, or production section.
2. Call `web_design_search_components` with the user's intent and an explicit limit to find a small candidate set.
3. Call `web_design_get_component_contract` only for the selected component.
4. Copy the returned library name, version, component, variant, props, default size, and editable slots exactly. Never invent a binding.

- Use Ant Design, Chakra UI, or shadcn/ui for mature product controls when a matching component exists.
- Use native geometry and typography for bespoke composition, backgrounds, decoration, and simple content.
- Do not silently mix design systems inside one section.
- Place editable children inside declared slots using both `parentId` and `slot`.
- Preserve page-local parent relationships and prevent cycles.
- Use semantic stable IDs such as `hero-heading` and `pricing-card-pro`.
- Treat `symbols` as shared definitions. Preserve instance IDs and `symbolOverrides`; synchronize instances after changing a definition.
- Use interactions for page navigation or HTTPS links only when the brief requires behavior.
- Prefer a small number of purposeful components over filling the canvas with every available control.
- A text node contains exactly one semantic text value. Do not format several controls or rows with spaces, tabs, pipes, indentation, Markdown, or newlines.
- A tool-size or parsing failure means the batch is too large. Reread the page, reduce the region, and retry with the same independent-node structure. Never replace the intended interface with descriptive prose.

Templates and section presets are optional accelerators and must remain editable component trees. Read [component examples](references/examples.md) before mixing library and custom composition.
