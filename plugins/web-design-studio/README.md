# Web Design Studio

Web Design Studio is an editable website design workbench and MCP server for ChatOS and Codex.

The product provides:

- a searchable, categorized drag-and-drop library with 20 layout, content, form, and data-display components;
- responsive ready-made Navbar, Hero, Features, Pricing, FAQ, Contact, and Footer sections;
- complete SaaS, product-launch, and business-service page templates that replace the current page in one action;
- an Apple-inspired workbench with system typography, neutral materials, translucent chrome, and a single system-blue accent;
- a scalable desktop canvas;
- independently scrollable library, canvas, and inspector regions, including long-page canvas scrolling;
- component selection, movement, resizing, deletion, and property editing;
- desktop, tablet, and mobile breakpoint-specific frames and typography overrides;
- a visual layer stack with visibility and locking controls;
- hierarchical component layers, shift multi-selection, grouping/ungrouping, and grouped dragging;
- canvas-edge and sibling snapping guides;
- container Flex row, Flex column, and Grid auto layout;
- multiple isolated pages with create, duplicate, rename, route, and delete controls;
- component-tree copy/paste within or across pages;
- image import into a reusable document asset library;
- standalone HTML export for the active responsive breakpoint;
- a reusable component library that preserves nested structures and responsive frames;
- reusable component instance synchronization with per-layer content, style, and frame overrides;
- updating component definitions from an instance, synchronizing all instances, and detaching an instance;
- global color, radius, and typography design tokens with CSS variable support;
- route switching while previewing multi-page designs;
- component click interactions that navigate to another page or open an external URL in preview mode;
- a routed single-file React JSX export;
- a routed single-file Vue SFC export;
- undo/redo, duplication, keyboard nudging, canvas alignment, and layer ordering;
- component-level annotations;
- component- or page-level AI request queues;
- revisioned JSON persistence shared by the visual workbench and MCP tools;
- focused AI patch operations that preserve unrelated user edits.

The component library includes sections, cards, dividers, headings, text, buttons, links, images, videos, icons, logos, inputs, textareas, selects, checkboxes, switches, badges, avatars, lists, and tables. Finished sections insert complete parent-child trees with desktop, tablet, and mobile frames so a usable page can be assembled without rebuilding common website patterns from primitives.

The left library is organized into Components, Sections, and Layers. The Sections view includes full-page templates and reusable user components; the canvas toolbar supports 25%–150% zoom, 100%, and fit-to-width controls.

The implementation plan is in [docs/IMPLEMENTATION_PLAN.zh-CN.md](docs/IMPLEMENTATION_PLAN.zh-CN.md).

## Run the workbench

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

Open `http://127.0.0.1:4187`. The Vite server proxies `/api` to the local workbench service on port 4188 when it is running, and otherwise falls back to browser-local storage.

## Run the MCP server

```bash
npm run build
node dist/mcp-server.mjs mcp
```

The MCP server exposes tools to list, create, read, patch, auto-layout containers, synchronize or update reusable component instances, export page HTML or routed React/Vue, validate structure, and process pending component-level AI requests. Focused patches can also manage pages, assets, design tokens, click interactions, instance overrides, and reusable component definitions.

## Persistence

Documents are stored in `WEB_DESIGN_STUDIO_DATA_DIR`, then `CHATOS_PLUGIN_DATA_DIR`, falling back to `.web-design-studio-data` in the current directory. The local workbench service and MCP server must receive the same data directory to collaborate on the same documents.

## Current AI interaction boundary

The visual button records an AI request in the shared design document. The conversation agent reads and handles that request through MCP. Direct model execution from inside the plugin iframe is intentionally deferred until the host provides a bounded `conversation.prompt` or `agent.run` bridge capability; the current host bridge supports context and artifact operations only.
