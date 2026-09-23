# Solution Studio

Solution Studio is an Apple-inspired project planning workbench and MCP server for turning existing-project evidence or a greenfield brief into traceable requirements, solution designs, and dependency-aware execution plans.

The execution plan is stored as a semantic DAG. Its graph is a visual projection of `tasks[].dependsOn`; moving a card changes only its saved canvas position, while changing a dependency is validated for missing references, self-dependencies, and cycles.

When ChatOS launches the plugin, Solution Studio reads the host-owned project context from `CHATOS_PROJECT_ID`, `CHATOS_PROJECT_NAME`, `CHATOS_WORKSPACE_ID`, `CHATOS_WORKSPACE`, and `CHATOS_CONTEXT_SCOPE`. The host project ID is never accepted as a tool or browser input. New solution workspaces are stamped with a `hostProject` binding and the store rejects a workspace bound to another active ChatOS project. `workspaceId` remains the Solution Studio document identity; `connectorWorkspaceId` names the ChatOS connector workspace.

## Local development

```bash
npm install
npm run build
npm run studio
```

Open `http://127.0.0.1:4198`. For UI development, run `npm run dev` on port `4197` with the local service running.

## Verification

```bash
npm run typecheck
npm test
npm run pack:verify
```
