---
name: web-design-progressive-generation
description: Plan and execute AI-first website design as resumable, visually verified bounded steps across page, overlay, and state artboards.
metadata:
  chatos.role: leaf
---

# Progressive website design

The editable visual Scene is the product. Code export and interactive-demo behavior are secondary.

## Required loop

1. Call `web_design_get_active_context`. Resume its plan and current step before proposing a new document or plan.
2. Use `web_design_plan_site` only for the site objective, audience, and page inventory. Planning multiple pages does not authorize generating them together.
3. Use `web_design_plan_page` for one artboard. Save art direction, composition, typography, imagery, content hierarchy, and concrete visual acceptance criteria before any Scene generation.
4. Start or resume one artboard with `web_design_start_page`. Starting an artboard does not mean finishing it in the same run. It may remain intentionally incomplete after a safe committed Step and be resumed from its current Scene and latest visual evidence later.
5. Execute only the single step named by `nextAction`. A complex artboard is expected to take many calls and sessions; split work by skeleton, semantic section, visual pass, responsive repair, or one bounded design problem. Never turn “one active artboard” into “finish this artboard now.”
6. Before a Candidate, call `web_design_capture_page` at every Step viewport. Use `web_design_capture_region` for the bounded target and `web_design_get_visual_grounding` to bind what is visible to stable node IDs. Pass only those returned current-revision artifacts as `visualInputs`; never invent artifact metadata.
7. After the Candidate verifier returns its candidate snapshot or crop, grounding, layout, calibration, quality report, and visual Diff, inspect those images before acceptance. After an auto-committed Step, capture the formal page again and call `web_design_compare_snapshots` against the pre-Step snapshot to confirm the actual committed impact.
8. When a user points at an image coordinate or the responsible layer is ambiguous, call `web_design_inspect_at_point`, then read only the smallest relevant candidate subtree. Do not ask the user to translate a visual location into a node ID.
9. Accept, reject, or retry only that Candidate. Never replay accepted steps. If human work is protected, show the field-level Diff and wait for explicit acceptance.

## Human annotation continuation

- When active context contains a `scene-annotation`, handle one annotation as one bounded design step. Do not fold several notes or pages into one transaction.
- Call `web_design_prepare_annotation_task` with the supplied document, node, annotation, and viewport. Its PNG crop is the visual brief; its task scope and Scene revision are the editable boundary.
- After editing, recapture the same target and compare snapshots. An operation succeeding is not enough to resolve the note; the requested visual difference must be visible in the returned image.
- If the Scene revision changed after preparation, discard the task context and prepare it again rather than rebasing from stale pixels.

## Visual evidence rules

- `web_design_capture_page` and `web_design_capture_region` return actual Chromium-rendered PNG content plus persistent artifacts. Look at the image; a path, node list, or successful render call is not visual review.
- Keep the clean PNG separate from grounding metadata. Grounding is for locating nodes and must not be painted over the image used to judge composition.
- Snapshot comparison requires the same Page and viewport. Treat changed regions outside the Step target as a scope failure until explained and reviewed.
- Artifact `revision` is authoritative. If the Scene revision changes, discard earlier visual inputs and capture again.
- At a Design Gate, inspect the whole page at every required viewport, not only the last edited Section.

## Design priority

- Make visual decisions before interaction decisions: hierarchy, composition, typography, color, imagery, spacing, material, rhythm, and responsive behavior.
- A working button, Drawer, form, or state transition is not evidence that the design is good.
- Interaction steps are optional unless the brief or the user explicitly needs them, and must follow the visual Design Gate.
- Use the current image as primary perceptual context and the bounded Scene subtree as precise editable context. Do not infer visual quality from node names alone.

## Visual-first Step contract

Every Step must solve one bounded visual problem and name the perceptual result it intends to create. A Step is not “build the page” or “make it functional.” For a substantial artboard, plan separate resumable Steps from this sequence, splitting any large phase again when needed:

1. **Art direction and composition skeleton** — establish the visual concept, content order, dominant geometry, whitespace, and focal path without filling every region.
2. **Primary focal region** — design the Hero or other first-attention region with deliberate type scale, imagery, contrast, and a clear visual anchor.
3. **Supporting rhythm** — add one semantic section or bounded group at a time while varying density, alignment, scale, and pacing intentionally.
4. **Visual-system application** — refine typography hierarchy, palette, spacing rhythm, radii, borders, surfaces, and signature details as a coherent system.
5. **Imagery and art treatment** — choose purposeful media, crop, layering, decoration, and background treatment that reinforce the audience and brand.
6. **Required viewport repair** — only for viewports in the brief, recompose hierarchy and density rather than merely shrinking the desktop result.
7. **Polish and Design Gate** — remove generic filler, accidental repetition, weak alignment, visual noise, and unresolved hierarchy before considering interaction.

This sequence defines visual responsibilities, not a requirement to complete one phase in one call. One phase may take several Steps, and AI must resume from the current Scene instead of rebuilding completed work.

## Screenshot acceptance rubric

Accept a visual Candidate only when the returned image demonstrates the Step's intended improvement. Review all of these that apply:

- A viewer can identify the primary focal point and reading order without consulting layer names.
- Type scale, contrast, alignment, spacing, and whitespace create clear hierarchy and comfortable rhythm.
- Composition has deliberate variation; it is not a stack of equal cards, equal rows, or generic component examples.
- Color, imagery, surfaces, and detail support a recognizable product, audience, industry, or brand direction.
- The edited region belongs with its neighbors and does not look pasted from a different component library.
- Required content is legible and the page does not feel crowded, empty by accident, clipped, or mechanically repeated.

If the screenshot does not meet the intended criterion, retry the same Step. A valid node tree, successful render, working control, or low Diff count cannot override a weak visual result.

## Anti-demo defaults

- Unless the brief explicitly describes an admin, analytics, operations, or data-management product, do not default to a sidebar, top utility bar, KPI tiles, dense tables, settings forms, or dashboard card grids.
- Do not fill space by repeating generic Card, Button, Input, Badge, and Avatar examples. Component libraries supply implementation-quality primitives; they do not supply the page's creative direction.
- Do not equate polish with gradients, glass panels, neon glows, or animation. Use effects only when they reinforce the chosen visual concept.
- Do not spend primary generation Steps on click handlers, form validation, loading logic, navigation wiring, or interaction demos. Design only the necessary visible states after the Design Gate, preferably as separate linked artboards.
- For expressive, marketing, editorial, portfolio, commerce, or brand sites, prioritize narrative flow, distinctive typography, imagery, composition, and memorable visual motifs over product-console conventions.

## Boundaries

- One tool call may advance at most one Step. An artboard is a durable design surface, not a unit of completion.
- At most one Step may be generating or validating at a time. Between committed Steps, AI may pause, wait for review, or deliberately focus another planned artboard without pretending the previous one is complete.
- Do not create all pages from a Site Plan, loop through remaining steps, or enlarge a transaction to reduce tool calls.
- Do not flatten a page into one image, one text node, or one opaque component.
- Use stable semantic node IDs. Keep changes inside the Step target subtree and preserve unrelated or manually adjusted work.
- A stale screenshot or Candidate must be discarded and rebuilt from the current Scene revision.
- Pause safely when the user needs to review; do not treat planning, successful tool calls, or interaction behavior as completion.
- Do not mark an artboard complete without a whole-artboard screenshot review at every viewport required by the brief and a passed visual Design Gate.
