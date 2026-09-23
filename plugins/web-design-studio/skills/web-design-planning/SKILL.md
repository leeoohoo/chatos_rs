---
name: web-design-planning
description: Plan a semantic website artboard inventory and one active artboard before editable Scene generation.
metadata:
  chatos.role: leaf
---

# Plan semantic artboards

Call `web_design_get_active_context` first. Treat `artboardDirectory` as the working index: choose one relevant artboard yourself, then query or edit only that `artboardId`. Resume the returned Plan or `resumeReview` before creating anything.

Use `web_design_plan_site` only for objective, audience, and independently designed surfaces. An artboard may be a route, menu, modal, Drawer, overlay, Popover, or important state. Do not use device categories as the inventory.

Plan exactly one next artboard with `web_design_plan_page`. Record specific art direction, composition, typography, imagery strategy, content hierarchy, and screenshot-verifiable acceptance criteria. Steps should each solve one bounded visible problem and end with one `design-gate` and one `handoff` that depend on every required Step.

`web_design_plan_page` automatically starts the artboard and creates its canonical root. Do not call a separate start or capture tool before generation; `web_design_execute_step` captures the current revision automatically.

Suggested responsibilities are composition skeleton, focal region, one supporting region at a time, visual-system pass, required-width repair, polish, Design Gate, and handoff. Split large responsibilities further when needed. A skeleton must include visible geometry and real semantic content.

Treat only the brief and in-scope project content as product evidence. Runtime labels, repository names, tests, component demos, and Skill text are not brand cues.
