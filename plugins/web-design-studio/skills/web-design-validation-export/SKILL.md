---
name: web-design-validation-export
description: Validate editable structure and screenshot-based visual quality, recover from revision conflicts, and export HTML, React, or Vue only when requested.
metadata:
  chatos.role: leaf
---

# Validation and export

`web_design_execute_step` renders and mechanically validates each Candidate before review. Inspect its returned screenshots and quality report, then accept or reject through `web_design_control_plan`. For the final visual Design Gate and handoff Steps, use the same Candidate loop with whole-artboard captures at every viewport required by the brief. Structural validation never proves that a design is visually finished.

Treat these as hard failures, not optional advice:

- `empty_page`
- `underbuilt_page`
- `page_as_text_mockup`
- `long_text_as_ui`
- `out_of_bounds`
- `children_outside_container`

When a Design Gate or handoff Candidate fails, use `web_design_query_scene` for the smallest flagged subtree, then call `web_design_execute_step` with the failed `stepId` and a visible repair for the stable issue IDs. Inspect the replacement screenshots. Do not refer to legacy document-validation or page-read tools that are not exposed by the Scene v2 workflow.

Before handoff, inspect real whole-artboard screenshots and reject completion when any of these remain:

- no clear focal point or reading order;
- weak typography hierarchy, contrast, alignment, spacing, or whitespace;
- generic dashboard or repeated-card composition not required by the brief;
- visual language that could belong to any unrelated product;
- inconsistent imagery, surfaces, density, or neighboring sections;
- clipped, crowded, accidentally empty, or mechanically repeated regions.

Capture and compare again after repair. A passing validator, successful tool response, component count, or working interaction cannot substitute for this visual review.

Verify queued requests are resolved only after their visual changes exist. Verify interactions only after the visual Design Gate, by previewing the separate destination artboard or HTTPS target when relevant. On revision conflict, reread and rebase rather than replacing user work.

Export or implement code only when the user asks for it and `deliveryGate.projectImplementationAllowed` is true. Return managed artifacts from the available export result and state what was generated. Export is secondary; the editable node tree remains the source of truth. Do not run dependency audits, automatic audit fixes, or unrelated package upgrades as a design-validation step.

Read [validation examples](references/examples.md) before declaring a large design complete.
