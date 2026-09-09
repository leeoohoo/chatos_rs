# Web Design Studio 3.0.1

Web Design Studio is an AI-first visual website design workbench and MCP server for ChatOS and Codex. The editable Scene is the product; interaction wiring and code export are optional handoff work.

## Product model

- AI plans the site, but generates only one active artboard and one bounded visual Step at a time.
- A complex page is expected to take several resumable Steps: composition, focal region, supporting sections, visual system, imagery, responsive repair, and polish.
- Every Step is reviewed with real Chromium screenshots, stable Scene grounding, layout diagnostics, visual Diff, and a Design Gate.
- People mainly review, annotate, lightly adjust, lock, or arrange layers. An annotation is bound to an exact Scene node, revision, and visual crop before AI edits it.
- Pages, modals, Drawers, popovers, menus, and important interface states are independent artboards. Scene nodes can link to them, and the workspace displays the relationship as a flow arrow.
- Responsive design uses the same Scene node tree with continuous layout rules and explicit tablet/mobile overrides. It does not create three copies of the page.

## Editing model

Scene v2 provides stable recursive nodes, exact revision history, transaction-based Undo/Redo, and field-level protection for human work. The workbench supports:

- selection through canvas or layers, including official component runtimes;
- move, eight-direction resize, multi-selection, marquee selection, copy/paste, delete, and keyboard nudging;
- grouping, Frame creation, Auto Layout Frame, ungrouping, alignment, equal distribution, and layer ordering;
- Auto Layout, Grid, free positioning, hug/fill/fixed sizing, constraints, content-sized artboards, and responsive child ordering;
- independent artboard creation, duplication, deletion, placement, zooming, panning, and full-workspace overview;
- Scene prototype targets with page navigation or in-page overlay preview;
- exact node annotations, screenshot-backed AI context, candidate review, retry, rejection, rollback, and protected-change approval.

## Component systems

The native toolbox intentionally contains only rectangle, ellipse, and line primitives. Product UI comes from separately organized official component systems:

- Ant Design 6
- Chakra UI 3
- shadcn/ui
- Magic UI
- Spell UI
- Inspira UI
- daisyUI 5

Each library keeps its own tab. A shared registry adapter handles catalog search, official source synchronization, isolated runtime loading, variant presentation, single-element selection, insertion, props, sample data, and editable content slots. The editor does not manufacture a fixed number of lookalike variants. Users choose one real official example before clicking or dragging it into the Scene.

Third-party licenses and excluded paid sources are documented in [docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md).

## AI workflow

The router Skill activates the progressive generation workflow for every new page or substantial redesign:

1. Read the active project/document context and resume the existing Plan.
2. Plan the site inventory without generating all pages.
3. Define visual direction and screenshot acceptance criteria for one artboard.
4. Start or resume that artboard.
5. Capture the current page or bounded region and visual grounding.
6. Generate one focused Candidate transaction.
7. Inspect its screenshots and Diff, then accept, reject, retry, or pause only that Step.
8. Run the whole-artboard Design Gate before interaction work or handoff.

The runtime rejects multi-page generation transactions, stale visual artifacts, out-of-scope edits, interaction wiring in visual Steps, and visual edits disguised as interaction Steps.

## Project scope and persistence

ChatOS supplies the project identity. The plugin derives an isolated storage scope from that transmitted context and keeps project membership, designs, Scene files, generation plans, screenshots, candidates, history, and workspace placement inside it. The host project ID is scope input and is never silently replaced by an unrelated user-selected project.

## Run locally

```bash
npm install
npm run build
npm run studio
```

Open `http://127.0.0.1:4188`.

For UI development:

```bash
npm run dev
```

The Vite development server uses port `4187` and proxies `/api` to the Studio service on `4188`.

## Verification

```bash
npm run typecheck
npm test
npm run pack:verify
```

The test suite covers project-scope isolation, Scene persistence and history, progressive planning and Candidate execution, screenshot artifacts, browser calibration, responsive layout, official component registries, Scene editing commands, prototype relations, and multi-website visual structure benchmarks.
