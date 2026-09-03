# Web Design Studio

Web Design Studio is an editable website design workbench and MCP server for ChatOS and Codex.

The product provides:

- a focused native shape toolbox containing only rectangle, ellipse, and line primitives;
- 68 real Ant Design 6.2.2 components across general, layout, navigation, data-entry, data-display, feedback, and utility categories;
- multiple curated variants for all 68 Ant Design components, including seven Input forms, six Select forms, six List forms, and real alternatives for navigation, layout, forms, data display, overlays, and feedback;
- an insertion-time live variant gallery, so people choose a visual treatment before adding a component;
- editable structured example data for options, items, table rows, trees, menus, steps, and other data-driven components;
- persisted Ant Design component identity, variant, and JSON-safe props, editable from the inspector and preserved in reusable components;
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
- six curated whole-site visual themes that also drive the live Ant Design theme;
- route switching while previewing multi-page designs;
- component click interactions that navigate to another page or open an external URL in preview mode;
- a routed single-file React JSX export;
- React export that imports and renders the original `antd` components instead of flattening them into lookalike markup;
- a routed single-file Vue SFC export;
- undo/redo, duplication, keyboard nudging, canvas alignment, and layer ordering;
- component-level annotations;
- component- or page-level AI request queues;
- a page-aware AI design command panel with quick prompts and automatic request persistence;
- an in-canvas interaction mode for typing, selecting, switching tabs, expanding panels, and opening overlays inside the designed webpage rather than across the editor shell;
- a true full-screen preview that removes the editor chrome, fits the website to the viewport, and keeps a small exit control;
- an MCP component-library catalog so AI can read supported components, variants, sample data, and themes before designing;
- revisioned JSON persistence shared by the visual workbench and MCP tools;
- focused AI patch operations that preserve unrelated user edits.

The native toolbox intentionally contains only rectangle, ellipse, and line primitives. Product UI comes from the actual Ant Design 6.2.2 package with 68 components, searchable in Chinese or English and insertable by click or drag. Components such as Input, Select, Button, Card, Alert, Progress, Tag, Avatar, and Badge expose selectable visual variants. Data-driven components ship with sample data that can be edited as structured JSON in the inspector.

The left library is organized into AntD, Shapes, Sections, and Layers. Clicking an Ant Design item opens a live variant gallery; dragging inserts its default variant immediately. The AntD runtime is code-split and loads only when an Ant Design component is present on the canvas. The Sections view includes full-page templates and reusable user components; the canvas toolbar supports 25%–150% zoom, 100%, and fit-to-width controls.

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

The MCP server exposes tools to list, create, read, patch, query the real component/theme catalog, auto-layout containers, synchronize or update reusable component instances, validate structure, and process pending page- or component-level AI requests. Export tools remain available for explicit delivery needs, while the primary workflow focuses on creating and refining the design itself.

## Persistence

Documents are stored in `WEB_DESIGN_STUDIO_DATA_DIR`, then `CHATOS_PLUGIN_DATA_DIR`, falling back to `.web-design-studio-data` in the current directory. The local workbench service and MCP server must receive the same data directory to collaborate on the same documents.

## Current AI interaction boundary

The visual button records an AI request in the shared design document. The conversation agent reads and handles that request through MCP. Direct model execution from inside the plugin iframe is intentionally deferred until the host provides a bounded `conversation.prompt` or `agent.run` bridge capability; the current host bridge supports context and artifact operations only.
