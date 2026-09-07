---
name: web-design-validation-export
description: Validate Web Design Studio structure and responsive quality, recover from revision conflicts, preview interactions, and export HTML, React, or Vue only when requested.
metadata:
  chatos.role: leaf
---

# Validation and export

Run `web_design_validate` with `mode: "draft"` after each logical region. Draft validation checks structural safety while allowing a page that is still being assembled. Run it with `mode: "handoff"` for each completed page and once for the complete document before reporting completion. Handoff mode rejects empty or underbuilt pages, a whole interface encoded in a large text node, excessive text used to simulate controls, blocking overflow, and invalid containment.

Treat these as hard failures, not optional advice:

- `empty_page`
- `underbuilt_page`
- `page_as_text_mockup`
- `long_text_as_ui`
- `out_of_bounds`
- `children_outside_container`

When handoff fails, call `web_design_get_page`, repair the flagged nodes in small component batches, and validate again. Export tools enforce the same handoff gate and will refuse a flattened or structurally invalid design.

Verify queued requests are resolved only after their visual changes exist. Verify interactions by previewing the destination or HTTPS target when relevant. On revision conflict, reread and rebase rather than replacing user work.

Export HTML, React, or Vue only when the user asks for code. Return managed artifacts from the export result and state what was generated. Export is a secondary artifact; the editable node tree remains the source of truth.

Read [validation examples](references/examples.md) before declaring a large design complete.
