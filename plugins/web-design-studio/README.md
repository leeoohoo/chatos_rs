# Web Design Studio

Web Design Studio is an editable website design workbench and MCP server for ChatOS and Codex.

The product provides:

- a focused native shape toolbox containing only rectangle, ellipse, and line primitives;
- all 72 components from the current Ant Design 6.6.2 official navigation, including Listy, BorderBeam, App, and ConfigProvider, grouped in the same seven categories as the website and backed by independently runnable official documentation demos;
- an independent Chakra UI 3.37.0 catalog containing 113 visual components and 1,152 independently runnable official composition examples, with no handwritten renderer fallback;
- an independent shadcn/ui catalog running all 61 official `new-york-v4` registry primitives, 300 resolvable official examples/blocks, and 14 declarative compositions for primitives whose upstream registry does not publish a standalone demo;
- an independent Magic UI catalog running all 68 currently downloadable MIT components from the official source, with all 123 downloadable official demo compositions and three direct-source fallbacks mounted through the shared React registry sandbox;
- an independent Spell UI catalog running all 33 components directly from its public MIT registry through the same React registry sandbox and a shared props/content protocol;
- an independent Inspira UI catalog running all 155 documentation entries and all 197 official Vue demos from the MIT-licensed upstream source through the shared sandbox adapter, without handwritten visual variants;
- an independent daisyUI 5.7.28 catalog covering all 68 current official component routes and all 587 official documentation examples, rendered from the upstream HTML structure with the official daisyUI CSS through a shared DOM registry adapter and loaded one component at a time;
- separate AntD, Chakra, shadcn, Magic, Spell, Inspira, and daisyUI tabs so component systems are never mixed into one palette;
- shared component-library and registry-runtime adapters for catalog definitions, official source synchronization, insertion, inspector props, editable slots, AI metadata, and canvas rendering dispatch;
- official Ant Design variant galleries whose counts and interactions come from the 6.6.2 documentation source rather than a fixed set of handcrafted lookalikes;
- an insertion-time live variant gallery, so people choose a visual treatment before adding a component;
- editable structured example data for options, items, table rows, trees, menus, steps, and other data-driven components;
- persisted UI-library identity, variant, and JSON-safe props, editable from the inspector and preserved in reusable components;
- 28 responsive ready-made sections grouped as navigation, hero, trust, product storytelling, editorial content, conversion, visual effects, and footers;
- eight complete page templates for SaaS, AI products, developer tools, product launches, enterprise services, creative studios, portfolios, and mobile apps;
- a personal My tab for saving selected components or grouped compositions as reusable design assets;
- an Apple-inspired workbench with system typography, neutral materials, translucent chrome, and a single system-blue accent;
- a scalable desktop canvas;
- independently scrollable library, canvas, and inspector regions, including long-page canvas scrolling;
- component selection, movement, resizing, deletion, and property editing;
- a library-agnostic visual style system for gradients, fill, stroke, radius, padding, shadow, foreground/background blur, opacity, transforms, overflow, blend modes, typography, and media cropping;
- separately editable default, hover, pressed, and focus visual states that work on native and library components in interactive preview;
- a personal “My” library where any selected component or multi-layer composition can be named, saved across designs, inserted again, renamed, or removed without affecting existing canvas instances;
- open-ended `customCss` declarations for visual effects that are not yet represented by a dedicated inspector control, while preserving validation, persistence, responsive overrides, and export;
- per-breakpoint minimum/maximum size constraints and aspect-ratio locking for predictable manual resizing and responsive reflow;
- desktop, tablet, and mobile breakpoint-specific frames and typography overrides;
- automatic tablet/mobile frame generation when inserting components, plus a toolbar action that fills missing responsive layouts without changing desktop geometry or existing hand-tuned overrides;
- a visual layer stack with visibility and locking controls;
- hierarchical component layers, shift multi-selection, grouping/ungrouping, and grouped dragging;
- canvas-edge and sibling snapping guides;
- container Flex row, Flex column, and Grid auto layout, including main-axis distribution and responsive row wrapping;
- multiple isolated pages with create, duplicate, rename, route, and delete controls;
- scope-isolated website projects keyed by the transmitted ChatOS project, workspace, user, and context identifiers, with stable default-project reuse and lossless legacy migration;
- URL-persisted internal project and design selection, preserving host query parameters and reopening the exact design after a full browser refresh;
- component-tree copy/paste within or across pages;
- image import into a reusable document asset library;
- standalone HTML export for the active responsive breakpoint;
- a reusable component library that preserves nested structures and responsive frames;
- reusable component instance synchronization with per-layer content, style, and frame overrides;
- updating component definitions from an instance, synchronizing all instances, and detaching an instance;
- global color, radius, and typography design tokens with CSS variable support;
- six curated whole-site visual themes that drive all live component-library renderers;
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

The native toolbox intentionally contains only rectangle, ellipse, and line primitives. These are open visual building blocks rather than a finite business-component catalog: the shared inspector can turn them and library components into gradients, glass surfaces, glows, masks through overflow, typographic treatments, transformed layers, and responsive compositions. Product UI comes from Ant Design, Chakra UI, shadcn/ui, Magic UI, Spell UI, Inspira UI, or daisyUI, each searchable in Chinese or English and insertable by click or drag. Ant Design definitions track the current official documentation URL, introduced version, and deprecation state; the legacy List remains available for existing work while new long-list designs use Listy. Common components expose selectable visual variants, and data-driven components ship with sample data that can be edited as structured JSON in the inspector.

The left library is organized into independent AntD, Chakra, shadcn, Magic, Spell, Inspira, daisyUI, Shapes, My, and Layers tabs. Clicking a library item opens a live variant gallery; dragging inserts its default variant immediately. Large galleries such as AntD Form/Table and shadcn Chart/Sidebar lazily mount official previews as the user scrolls, avoiding hundreds of hidden runtimes. Runtime adapters are loaded by library on demand, and every demo module is code-split. Cross-framework source libraries use an isolated adapter protocol: a sync step consumes the upstream registry, the framework-specific sandbox mounts the real component, and the editor only sends props and receives lifecycle/events. Official primitives without a published demo use a library-neutral declarative composition tree that references their real exported components rather than a handwritten lookalike renderer. Content-bearing components share the same nested-canvas contract, while default form/detail starters preserve the selected parent's library instead of mixing systems. The canvas toolbar supports 25%–150% zoom, 100%, and fit-to-width controls.

Third-party license details and explicit exclusions are recorded in [docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md). Magic UI Pro templates and React Bits components are not bundled.

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

The host `CHATOS_PROJECT_ID` is never reused as an internal filename. It participates in a stable runtime-scope fingerprint together with the context, workspace, user/account, and resolved data directory. Projects and designs are listed and opened only inside that scope; the generated internal default project ID is returned through `/api/context`. Existing unscoped projects are assigned to the first scope without rewriting their design files.

## Current AI interaction boundary

The visual button records an AI request in the shared design document. The conversation agent reads and handles that request through MCP. Direct model execution from inside the plugin iframe is intentionally deferred until the host provides a bounded `conversation.prompt` or `agent.run` bridge capability; the current host bridge supports context and artifact operations only.
