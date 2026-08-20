# Okra

English · [简体中文](./README.zh-CN.md)

> Bring AI into your project—and get things done.

Okra is the product name of Chat OS and an AI work partner built for real projects.

It does more than answer questions. Okra can discuss requirements with you, understand project context, break down work, read code, use tools, run tasks, and turn important information into reusable project memory.

Ask simple questions directly, or hand complex work to background tasks. You can return at any time to review progress, tool output, code changes, and final results without explaining the entire project again.

## What Okra Can Do

### Turn rough ideas into executable plans

Start with a plain-language request and work with Okra to clarify the goal, constraints, and acceptance criteria.

A project plan can bring together:

- Product requirements and business goals
- Technical proposals and project documentation
- Tasks that can be executed directly
- Dependencies between tasks
- Current progress, failure reasons, and follow-up work

A plan is not a disposable chat response. Once confirmed, its related tasks can move directly into execution.

### Let AI work inside a real project

Okra works in the right engineering environment, so you do not have to keep copying code and commands into a chat window.

Depending on the project type, it can use:

- Project files and full-text search
- Git status, branches, diffs, commits, and synchronization
- Terminals and long-running commands
- Browser automation, code maintenance, and other engineering tools
- Project languages, toolchains, and environment variables
- An isolated cloud environment or a local directory you have authorized

Every operation stays connected to the current project and leaves a process you can review later.

### Hand complex work to background tasks

When a request requires many steps, Okra can send it to the task system instead of making you stay in the chat window.

You can see:

- What is being worked on now
- Which tasks are completed, running, blocked, or failed
- Which tools the AI used
- Commands, runtime logs, and code changes
- Whether Okra needs more information or your confirmation
- Success, stop, failure, and retry results

Task lifecycles are managed in the cloud and can continue after you leave the page. Tasks that need local files, commands, plugins, MCP servers, or permission-controlled device capabilities require the desktop client and Local Connector to remain online.

### Remember the project, not just one conversation

Okra continuously organizes project context, important decisions, conversation summaries, and role-specific memory so long-term collaboration is not limited to a single chat.

This lets you:

- Continue previous work in a new conversation
- Let the AI remember project conventions and personal preferences
- Review the summary accumulated in the current conversation
- Recall important information from related conversations
- Explicitly forget a Recall that is no longer useful in the current project

Memory is not an endless pile of chat history. Okra uses summaries and layered organization to keep the information that is most useful for future work.

### Create AI partners for different projects

Different projects benefit from different ways of working. You can create several agents with clear responsibilities, then choose one as the current contact for each project. Chat, project context, and background tasks will use that contact by default.

For example:

- A product partner focused on requirement clarification
- A development partner familiar with a specific technology stack
- An engineering partner focused on testing, debugging, or code review
- A project partner responsible for maintaining documentation and task status

Each agent can have its own role, boundaries, model, skills, and tool capabilities. You can switch the project's contact when it has no task currently running.

## Where Okra Fits

### Start a new project from scratch

Tell Okra what you want to build. It can help shape the requirements, technical approach, task dependencies, and acceptance criteria before moving through the implementation plan.

### Take over or maintain an existing codebase

Import a Git project or authorize a local directory. Okra can read the project structure before working on features, refactoring, tests, dependency upgrades, or bug fixes.

### Move long-running, complex work forward

Hand multi-step work to the task system and review execution status, failure reports, retries, and final deliverables in one place instead of relying on an untraceable long conversation.

### Maintain a long-term personal project

Let project context, past decisions, pending work, and conversation memory accumulate over time. Return days or weeks later and continue from the context already available.

### Choose the right role for each project

Create product, development, testing, or research contacts, then choose the most appropriate one for an ongoing project.

## Get Started in Three Steps

### 1. Create a project

Install the desktop client, register a Local Connector workspace, and create the project from an authorized directory. Every project uses this workspace for files, Git, search, and commands.

### 2. Tell Okra what you want to accomplish

Describe the goal directly, and add any constraints, source material, or acceptance criteria you already have.

For more complex work, enter Plan mode so Okra can organize requirements, documents, tasks, and dependencies before execution begins.

### 3. Follow the process and keep collaborating

While a task is running, you can review progress, send additional guidance, answer Okra's questions, stop the task, or retry after a failure.

When the work is complete, the result, project changes, and suggested next steps return to the same conversation and project context.

## Projects and Workspaces

Okra has one project model. Every project binds an authorized Local Connector workspace; there is no cloud/local project type switch.

- Project, session, message, task, requirement, memory, and Agent lifecycles are orchestrated by server-side services.
- Project files, Git, search, commands, local Skills/Plugins/MCP servers, permission relay operations, and approvals execute through Local Connector Client.
- Harness can manage repository assets, branches, synchronization, CI, and integrations, but it is never used as the MCP project file or command provider.
- If Local Connector is unavailable, workspace operations fail or wait explicitly. They do not fall back to a server filesystem, Harness, or another execution host.

### Projects and privacy

Keep in mind:

- Okra Cloud stores only logical workspace and device routing identifiers, not the absolute path of your local workspace.
- Cloud services are the single source of truth for business data; the client does not keep a second copy of sessions, tasks, or memory.
- Content needed for AI inference may still be sent to the model provider you choose. Review that provider's data policy as well.
- Control information such as your account, agent capabilities, model catalog, and system policies may synchronize with your account.
- Terminal, file, and Git operations are restricted to the authorized workspace boundary.

## What You Will Find in Okra

### Conversation space

Work continuously with the project contact and review AI responses, reasoning stages, tool activity, task status, and message history.

### Project plans

Review requirements, technical documents, project tasks, dependencies, and execution status in one place, then launch related work directly from a requirement.

### Project workspace

Browse and search files, inspect Git changes, edit project content, configure how the project runs, and start or inspect project instances.

### Task center

Review background tasks, run history, human confirmations, tool status, successful results, and failure reasons.

### Memory view

Review conversation summaries and recallable memory, run retrospectives, and manage automatic summaries and Recalls for projects.

### Agents and capabilities

Create agents, choose models, enable the tools and skills they need, and set default models and reasoning levels for different kinds of work.

## Before You Begin

### Creating a project

1. Download and install the Okra desktop connector from the Okra website.
2. Sign in with the same account you use on the web.
3. Add and authorize a local workspace.
4. Configure the local tools, Skills, Plugins, MCP servers, permission controls, and approval permissions you need.
5. Create the project from that workspace in the desktop client.
6. Add a project contact, then start a conversation or plan.

Opening Okra in a regular browser does not grant access to directories on your computer. Workspace operations require the desktop client and Local Connector to be online.

## Frequently Asked Questions

### How is Okra different from a typical AI chat tool?

Typical chat tools primarily generate responses. Okra is designed for continuous project collaboration: it can connect to project environments, use tools, manage plans and tasks, report progress, and reuse project context accumulated during earlier conversations.

### Do I have to upload my code to the cloud?

No. Project code remains in the directory authorized through Local Connector. Content needed for AI inference may still be sent to the model provider you configured.

### Can I use my own model service?

Yes. Okra supports OpenAI-compatible model services and lets you select different models for general chat, project planning, memory summaries, and task execution.

### Can I intervene while the AI is working?

Yes. You can review tool activity, respond to confirmation requests, send additional guidance, stop the current run, and retry after a failure.

### Will a task continue after I close the page?

Task orchestration and business state continue on the server. When the next step needs project files, commands, or another device capability, the desktop client and Local Connector must be online.

### Can a project fall back to a cloud workspace when my device is offline?

No. Project workspace access only uses the bound Local Connector. MCP Management never silently switches to Harness, a server filesystem, another execution host, or another device.

## Current Product Status

Okra is evolving quickly. Current limitations include:

- Project workspace access requires the desktop client and an online Local Connector.
- Projects do not yet support chat attachments or image/file attachments in additional guidance sent while a task is running.
- Business history is stored by server-side services. Version 2.0.10 does not migrate historical sessions, tasks, or memory from the legacy client SQLite database.
- Available desktop platforms, versions, and registration rules depend on the Okra deployment you use.

## Technical and Self-Hosting Reference

The following section is for maintainers who deploy, debug, or extend Okra. Regular users do not need it.

<details>
<summary>Expand architecture notes and development commands</summary>

### Execution architecture

Okra uses one cloud business orchestration plane plus device-side capability executors:

- Project, conversation, task, requirement, memory, and Agent lifecycle data is authoritative in cloud services.
- Local Connector Core only executes capabilities that must run on the user's device, including workspace files, Git, commands, local Skill/Plugin/MCP components, permission-controlled task leases, and approvals.
- MCP Management routes project files, Git, search, and commands only to the bound Local Connector. Harness remains a repository and integration control plane.
- Local Connector unavailability fails explicitly; it never moves a device-scoped operation to another execution host silently.

### Local data paths

The default Local Connector state paths are:

```text
~/.chatos/local_connector/state.json
~/.chatos/local_connector/connector-state.sqlite3
```

Set `LOCAL_CONNECTOR_STATE_PATH` to change the state file location. The SQLite database stores only connector control state in the same directory. Legacy `runtime.sqlite3` files are not migrated or used by 2.0.10.

### Start the self-hosted cloud stack

Docker Engine and Docker Compose v2 are required:

```bash
cp docker/bootstrap.conf.example docker/bootstrap.conf
make docker-up
```

The main application is available at <http://localhost:8088> by default. Business configuration is published through Configuration Center. `docker/bootstrap.conf` contains only the infrastructure and credentials required before Configuration Center can be reached and must not be committed.

Build images from the current source:

```bash
make dev
```

Host-side development mode:

```bash
make local-dev
make local-dev-status
make local-dev-logs SERVICE=chatos-backend
make local-dev-stop
```

### Local Connector development

Start the Core service and settings page:

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

The complete project workspace experience depends on the trusted Runtime Bridge provided by Electron. Core/settings development mode alone is not equivalent to the complete desktop client.

Package for macOS:

```bash
./local_connector_client/package-electron-macos-client.sh
```

Package for Windows:

```powershell
powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\package-electron-windows-client.ps1
```

Package the Linux core profile on Linux:

```bash
./local_connector_client/package-electron-linux-client.sh
```

The Linux package includes the desktop client, Local Connector Core, and the verified Plugin/Skill
catalogs. Browser automation, Chrome Native Messaging, Computer Use, and the
bundled document runtime remain excluded until Linux-native runtime assets are available.

### Build and test

```bash
make build
make smoke
make test
```

The main services can also be tested independently:

```bash
cargo test -p chat_app_server_rs
cargo test -p task_runner_service_backend
cargo test -p local_connector_client_core
cd memory_engine/backend && cargo test
```

### Architecture sources of truth

- Deployment boundaries and ports: `docker/compose.yml`
- Rust workspace: `Cargo.toml`
- Cloud business APIs and the local capability bridge: `chatos/frontend/src/lib/api/client/facades/`
- Cloud execution boundary: `chatos/backend/src/core/project_execution.rs`
- Local Connector capability executor: `local_connector_client/core/src/local_runtime/`
- Local Connector control-state schema: `local_connector_client/core/migrations/`
- Cloud Task Runner: `task_runner_service/backend/src/services/`
- Development and deployment commands: `Makefile`, `docker/deploy.sh`, and `scripts/local-dev-stack.sh`
- `chatos_3d_anime_prototype/` is an experimental interface, not the current production entry point.

</details>

## License

This project is licensed under the [PolyForm Noncommercial License 1.0.0](./LICENSE). See [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md) for third-party notices.
