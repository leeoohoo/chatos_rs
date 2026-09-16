---
name: web-design-candidate-review
description: Review real Candidate and Diff images, control Plan state, and verify human annotation changes.
metadata:
  chatos.role: leaf
---

# Review visible output

Inspect every Candidate and Diff PNG returned by `web_design_execute_step`. Judge focal point, hierarchy, typography, spacing, composition, brand relevance, legibility, and the absence of clipping, overlap, or accidental emptiness. Layout and browser calibration passing does not prove the design is good.

Use `web_design_control_plan`:

- `accept` commits the reviewed Candidate. If it is the handoff Step, the program also completes the artboard and returns a compact checkpoint.
- `reject` records the visual reason and discards the Candidate. Then call `web_design_execute_step` with corrected operations.
- `skip` is only for an optional Step.
- `rollback` restores only the latest accepted Step when safe.
- `pause` and `resume` act at safe Step boundaries.

When `web_design_get_active_context` returns `resumeReview`, inspect its replayed images and decide directly; do not request the same Candidate again.

For a human Scene annotation, use `web_design_prepare_annotation_task`, treat its crop as the visual brief, make the smallest revision-checked edit, recapture the same region, and compare the snapshots. Resolve the note only after the requested difference is visible. If the Scene revision changed, prepare the annotation again.

At Design Gate and handoff, inspect the whole active artboard at every width required by the brief. Candidate acceptance remains a perceptual AI decision and must not be automated.
