# Design content types

A `DesignSection` is the single decided detailed design for a requirement. Its `body` is a short summary; its `blocks` carry the actual design detail. Give every block a stable ID scoped to its solution, such as `D-001-B-001`. Project-wide blocks use `D-000-B-*`.

## Shared output contract

```json
{
  "id": "D-001-B-001",
  "type": "text | architecture | flowchart | ui-svg",
  "title": "Human-readable title",
  "content": "Markdown text or standalone SVG source"
}
```

- `text` stores readable Markdown in `content`.
- Every visual type stores raw SVG source in `content`, beginning with an `<svg>` element. Do not wrap it in a Markdown code fence.
- SVG must be self-contained: include a `viewBox`, use inline presentation attributes or an internal `<style>`, and use only local shapes, paths, gradients, markers, and text.
- Do not use scripts, event handlers, `foreignObject`, remote URLs, external fonts, external images, plugin identifiers, or links to another workspace.
- Prefer a light neutral canvas, system-font fallbacks, restrained Apple-style color, clear spacing, and legible labels. Use dark text on light surfaces unless the requested design calls for a dark appearance.
- Escape XML-sensitive text. Keep important labels as SVG `<text>` so the artifact remains inspectable.
- Make arrow direction and reading order unambiguous. Avoid crossed connectors when a small layout change can remove them.
- Route every connector from one shape boundary to another. Reserve enough clear space for the full marker plus visible padding; a short segment must not place its arrowhead inside either node.
- Do not run a connector, arrowhead, or branch label through a node's bounding box. Give incoming, outgoing, and return paths separate ports or lanes when sharing an edge would create overlap.
- Before persisting, verify that the SVG parses, its content fits inside the `viewBox`, labels are not clipped, and it remains understandable at the plugin preview width.
- Render the finished SVG at the plugin preview width and visually inspect every junction, decision branch, return loop, and arrowhead. Coordinate checks alone are insufficient.

## `text`

Use for intent, behavior, constraints, tradeoffs, interfaces, data contracts, edge cases, rollout, and validation notes. Prefer short sections and concrete statements tied to requirement IDs. Distinguish verified project facts, user-stated facts, and assumptions.

Do not put ASCII diagrams or an SVG code fence in a text block. Create the corresponding visual block instead.

## `architecture`

Generate a system or component architecture as SVG code.

- Show trust, process, service, package, or deployment boundaries when they affect the decision.
- Label components with concrete names from the inspected project where evidence exists.
- Use labeled connectors to distinguish calls, events, and data movement.
- Include stores, external systems, error paths, and ownership boundaries only when relevant.
- Add a compact legend when color or line style has semantic meaning.
- The diagram should explain responsibilities and dependencies, not merely list components.

A useful default is a left-to-right layout with related components grouped into softly tinted regions. Prefer rounded rectangles, thin separators, and whitespace over card-heavy decoration.

## `flowchart`

Generate a behavioral, business, state, or data flow as SVG code.

- Include a clear start and terminal outcome.
- Use diamonds only for real decisions and label outgoing branches.
- Show failure, retry, cancellation, or fallback paths when they matter to the requirement.
- Keep the primary path visually dominant and place exceptional paths below or to the side.
- This is a design flow, not the execution-plan DAG. Implementation ordering belongs in `executionPlan.tasks[].dependsOn` and is rendered separately.

## `ui-svg`

Generate an interface design or wireframe as SVG code, not a raster screenshot.

- Choose a concrete viewport and include the full screen or window chrome needed to understand context.
- Reflect the actual hierarchy, copy, controls, states, spacing, and primary action required by the user flow.
- Follow Apple platform conventions: calm surfaces, strong typography hierarchy, generous whitespace, subtle separators, compact controls, and limited accent color.
- Show the important normal, empty, loading, validation, or error state when the requirement depends on it. Use separate blocks for substantially different screens or states.
- Use reusable SVG groups for repeated elements. Keep all text editable as `<text>` and all geometry vector-based.
- Do not embed screenshots, base64 raster images, HTML, or `foreignObject`.

## Choosing blocks

- Back-end or infrastructure change: text plus architecture; add flowchart for multi-step or failure-sensitive behavior.
- User-facing feature: text plus flowchart and `ui-svg`; add architecture when system boundaries or data contracts change.
- Small isolated change: text alone may be sufficient.
- Materially different approaches: choose one in the design, then record the rejected approaches and reasoning in `decisions[].alternatives`; do not create parallel design sections for one requirement.
- Core product, technical, system, or game-mechanic candidate: text plus at least one requirement-specific architecture or flowchart. Text alone is incomplete.
- Explicit visualization request: every primary candidate needs its own relevant visual block. A diagram attached to a different requirement does not count.

Before calling `solution_upsert_design`, build a short requirement-to-block audit and check each primary candidate for: explanatory text, the required architecture or flow, any requested interface SVG, stable block IDs, and valid standalone SVG. After the call, read the saved workspace and verify those block IDs and types exist.
