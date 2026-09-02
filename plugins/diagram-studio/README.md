# Diagram Studio

Diagram Studio is an Apple-inspired visual workbench and MCP server for:

- software architecture diagrams;
- flowcharts;
- swimlane diagrams;
- infrastructure and network topology maps;
- sequence diagrams;
- bidirectional PlantUML source synchronization for all five diagram types.

The same structured JSON document is designed for direct user editing and focused AI patch operations.

The workbench includes five diagram-specific component libraries, a searchable drag-and-drop library of common shapes
and semantic application, data, infrastructure, and process icons, node and edge inspectors, automatic layout,
independent text objects, eight-direction node resizing, editable node borders and edge line styles,
undo/redo, revisioned persistence, direct vertical sequence-message movement, and PNG/SVG/JSON/PlantUML export.

PlantUML supports bidirectional Sequence, Activity, Component, and Deployment diagrams. Sequence covers participants,
messages, activations, and combined fragments; Activity covers actions, decisions, branches, terminal states,
and partition-based swimlanes; Component covers actors, services, interfaces, databases, queues, and dependencies;
Deployment covers nodes, clouds, storage, artifacts, databases, and infrastructure links. Diagram Studio embeds optional
layout data in ordinary PlantUML comments so its own exports can restore canvas positions and styling,
while other PlantUML tools continue to read the file normally.

## Run the visual workbench

```bash
npm install
npm run build
npm run studio
```

Open `http://127.0.0.1:4178`.

Documents are stored in `DIAGRAM_STUDIO_DATA_DIR`, then `CHATOS_PLUGIN_DATA_DIR`, falling back to `.diagram-studio-data` in the current directory.

For UI-only development with browser-local storage:

```bash
npm run dev
```

Open `http://127.0.0.1:4177`.

## MCP server

```bash
npm run build
node dist/mcp-server.mjs mcp
```

The server publishes tools for listing, creating, reading, patching, laying out, validating, importing PlantUML source for all five diagram types, and exporting diagram documents.

## Keyboard shortcuts

- `⌘S`: save
- `⌘Z`: undo
- `⇧⌘Z`: redo
- `Delete`: remove the selected node or edge

## Security boundary

The visual page does not receive model credentials or arbitrary filesystem access. In standalone mode it talks only to the loopback Diagram Studio service. In ChatOS it is intended to run inside the sandboxed Plugin App host and use a bounded host bridge.
