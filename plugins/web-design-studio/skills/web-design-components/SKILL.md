---
name: web-design-components
description: Build editable Web Design Studio component trees with supported libraries, slots, hierarchy, symbols, interactions, and stable semantic IDs.
metadata:
  chatos.role: leaf
---

# Component systems

Call `web_design_get_component_library` before introducing product UI or a new component family. Copy supported library name, version, component, variant, props, sample data, and editable slots from the returned contract.

- Use Ant Design, Chakra UI, or shadcn/ui for mature product controls when a matching component exists.
- Use native geometry and typography for bespoke composition, backgrounds, decoration, and simple content.
- Do not silently mix design systems inside one section.
- Place editable children inside declared slots using both `parentId` and `slot`.
- Preserve page-local parent relationships and prevent cycles.
- Use semantic stable IDs such as `hero-heading` and `pricing-card-pro`.
- Treat `symbols` as shared definitions. Preserve instance IDs and `symbolOverrides`; synchronize instances after changing a definition.
- Use interactions for page navigation or HTTPS links only when the brief requires behavior.

Templates and section presets are optional accelerators and must remain editable component trees. Read [component examples](references/examples.md) before mixing library and custom composition.
