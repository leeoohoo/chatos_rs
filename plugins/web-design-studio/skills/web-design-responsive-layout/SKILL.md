---
name: web-design-responsive-layout
description: Create and repair Web Design Studio desktop, tablet, and mobile layouts with containers, constraints, Flex/Grid, auto layout, and bounded viewport behavior.
metadata:
  chatos.role: leaf
---

# Responsive layout

Use responsive behavior only for viewports required by the brief; do not create device artboards by default. Breakpoint views are visual checks of the same Scene artboard, while distinct destinations, overlays, menus, and interface states remain separate artboards. Set Scene hierarchy before fine positioning. Use Scene `layout.mode` (`free`, `auto`, or `grid`), sizing, constraints, padding, gap, and responsive rules inside one bounded visual-repair Step.

- Use `responsiveRules` for required width ranges and keep node overrides bounded to the affected page.
- Use Scene constraints, sizing modes, and min/max dimensions for container behavior.
- Keep children and parents on the same page.
- Check content inside the viewport and containers; repair clipping, overflow, accidental overlap, and unreadable density.
- Mobile is a recomposed layout, not a uniformly shrunken desktop canvas.
- Preserve the art direction, focal order, typography intent, and visual rhythm across required viewports; structural fit alone is insufficient.
- Preserve deliberate user positioning unless the request or validation requires repair.

Read [responsive examples](references/examples.md) before a substantial new page or breakpoint repair.
