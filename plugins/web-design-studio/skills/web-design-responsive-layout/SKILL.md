---
name: web-design-responsive-layout
description: Create and repair Web Design Studio desktop, tablet, and mobile layouts with containers, constraints, Flex/Grid, auto layout, and bounded viewport behavior.
metadata:
  chatos.role: leaf
---

# Responsive layout

Set page breakpoints and component hierarchy before fine positioning. Use `set_layout` for `free`, `flex-row`, `flex-column`, or `grid` containers, then run `web_design_auto_layout` for each affected device.

- Pass `device` for breakpoint-specific move, resize, style, or frame changes.
- Use constraints for horizontal behavior, min/max dimensions, and aspect-ratio locking.
- Keep children and parents on the same page.
- Check content inside the viewport and containers; repair clipping, overflow, accidental overlap, and unreadable density.
- Mobile is a recomposed layout, not a uniformly shrunken desktop canvas.
- Preserve deliberate user positioning unless the request or validation requires repair.

Read [responsive examples](references/examples.md) before a substantial new page or breakpoint repair.
