---
name: web-design-validation-export
description: Validate editable structure and screenshot-based visual quality, recover from revision conflicts, and export HTML, React, or Vue only when requested.
metadata:
  chatos.role: leaf
---

# Validation and export

Run `web_design_validate` with `mode: "draft"` after each bounded Step. Draft validation checks structural safety while allowing an artboard that is intentionally incomplete and will be resumed later. Structural validation never proves that a design is visually finished. Run `mode: "handoff"` only after the visual Design Gate passes for the whole artboard at every viewport required by the brief, then once for the complete document before reporting completion. Handoff mode rejects empty or underbuilt pages, a whole interface encoded in a large text node, excessive text used to simulate controls, blocking overflow, and invalid containment.

Treat these as hard failures, not optional advice:

- `empty_page`
- `underbuilt_page`
- `page_as_text_mockup`
- `long_text_as_ui`
- `out_of_bounds`
- `children_outside_container`

When handoff fails, call `web_design_get_page`, repair the flagged nodes in small component batches, and validate again. Export tools enforce the same handoff gate and will refuse a flattened or structurally invalid design.

Before handoff, inspect real whole-artboard screenshots and reject completion when any of these remain:

- no clear focal point or reading order;
- weak typography hierarchy, contrast, alignment, spacing, or whitespace;
- generic dashboard or repeated-card composition not required by the brief;
- visual language that could belong to any unrelated product;
- inconsistent imagery, surfaces, density, or neighboring sections;
- clipped, crowded, accidentally empty, or mechanically repeated regions.

Capture and compare again after repair. A passing validator, successful tool response, component count, or working interaction cannot substitute for this visual review.

Verify queued requests are resolved only after their visual changes exist. Verify interactions only after the visual Design Gate, by previewing the separate destination artboard or HTTPS target when relevant. On revision conflict, reread and rebase rather than replacing user work.

Export HTML, React, or Vue only when the user asks for code. Return managed artifacts from the export result and state what was generated. Export is a secondary artifact; the editable node tree remains the source of truth.

Read [validation examples](references/examples.md) before declaring a large design complete.
