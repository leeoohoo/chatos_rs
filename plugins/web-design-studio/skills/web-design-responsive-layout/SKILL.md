---
name: web-design-responsive-layout
description: Create and repair responsive behavior inside one Web Design Studio semantic artboard using containers, constraints, Flex/Grid, auto layout, and required viewport ranges.
metadata:
  chatos.role: leaf
---

# Responsive layout

Use responsive behavior only for viewport widths required by the brief. A breakpoint view is a visual check of the same semantic Scene artboard, not a new desktop, tablet, or mobile artboard. Distinct destinations, expanded menus, modals, Drawers, overlays, Popovers, and important interface states remain separate artboards because they represent different user-visible surfaces. Set Scene hierarchy before fine positioning. Use Scene `layout.mode` (`free`, `auto`, or `grid`), sizing, constraints, padding, gap, and responsive rules inside one bounded visual-repair Step.

- Use `responsiveRules` for required width ranges and keep node overrides bounded to the affected page.
- Use Scene constraints, sizing modes, and min/max dimensions for container behavior.
- Keep children and parents on the same page.
- Check content inside the viewport and containers; repair clipping, overflow, accidental overlap, and unreadable density.
- A responsive Candidate is not complete while mechanical verification reports `containment:*` or `overlap:*` issue IDs. Use those stable container/child IDs to make a bounded retry, then inspect the replacement screenshots at every required viewport.
- At a required narrow width, recompose hierarchy and density instead of uniformly shrinking the wider result.
- Preserve the art direction, focal order, typography intent, and visual rhythm across required viewports; structural fit alone is insufficient.
- Preserve deliberate user positioning unless the request or validation requires repair.

Never generate three device-labeled artboards as a responsive implementation. Create separate artboards only for semantic destinations or states, or when the user explicitly asks for independent visual variants.

Read [responsive examples](references/examples.md) before a substantial new page or breakpoint repair.
