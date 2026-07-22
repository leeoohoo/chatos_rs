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

Cloud tasks can continue after you leave the page. Local tasks require the desktop client's local runtime to remain online.

### Remember the project, not just one conversation

Okra continuously organizes project context, important decisions, conversation summaries, and role-specific memory so long-term collaboration is not limited to a single chat.

This lets you:

- Continue previous work in a new conversation
- Let the AI remember project conventions and personal preferences
- Review the summary accumulated in the current conversation
- Recall important information from related conversations
- Explicitly forget a Recall that is no longer useful in a local project

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

You can:

- Create a cloud project from a Git repository
- Upload a ZIP archive as a cloud project
- Select an authorized local directory in the desktop client

### 2. Tell Okra what you want to accomplish

Describe the goal directly, and add any constraints, source material, or acceptance criteria you already have.

For more complex work, enter Plan mode so Okra can organize requirements, documents, tasks, and dependencies before execution begins.

### 3. Follow the process and keep collaborating

While a task is running, you can review progress, send additional guidance, answer Okra's questions, stop the task, or retry after a failure.

When the work is complete, the result, project changes, and suggested next steps return to the same conversation and project context.

## Cloud or Local Projects

Okra does not force every project into the same working model.

| | Cloud project | Local project |
| --- | --- | --- |
| Best for | Browser access, isolated environments, or cloud execution that keeps running | Code that stays on your computer and uses local directories and toolchains |
| Created from | A Git repository or ZIP archive | A local directory authorized in the desktop client |
| Working environment | A cloud Git workspace and isolated runtime | A local workspace authorized by you |
| Access | Browser or desktop client | Desktop client |
| Task execution | Cloud background tasks | Local task service on the current device |
| Project memory | Stored in the cloud | Stored on the current device |

The execution location is explicit. A local project will not silently fall back to the cloud when the local service is unavailable, and a cloud project will not unexpectedly start running on your computer.

### Local projects and privacy

Files, conversations, tasks, plans, and memory for a local project are managed by the desktop client's local runtime. Every project directory must be explicitly authorized by you.

Keep in mind:

- Okra Cloud does not store the absolute path of your local workspace.
- A local project is not automatically copied to the cloud or written to both execution environments.
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

Review conversation summaries and recallable memory, run retrospectives, and manage automatic summaries and Recalls for local projects.

### Agents and capabilities

Create agents, choose models, enable the tools and skills they need, and set default models and reasoning levels for different kinds of work.

## Before You Begin

### Using a cloud project

1. Open the Okra website for your deployment.
2. Register or sign in. Some test deployments may require an invitation code.
3. Add an available cloud AI model in Settings.
4. Create or import a cloud project.
5. Add a project contact, then start a conversation or plan.

### Using a local project

1. Download and install the Okra desktop connector from the Okra website.
2. Sign in with the same account you use on the web.
3. Add and authorize a local workspace.
4. Configure the local model, tools, skills, and system permissions you need.
5. Create a local project in the desktop client.

Local projects are available only in the desktop client. Opening Okra in a regular browser does not grant it access to directories on your computer.

## Frequently Asked Questions

### How is Okra different from a typical AI chat tool?

Typical chat tools primarily generate responses. Okra is designed for continuous project collaboration: it can connect to project environments, use tools, manage plans and tasks, report progress, and reuse project context accumulated during earlier conversations.

### Do I have to upload my code to the cloud?

No. When you create a local project in the desktop client, the code can remain in the local directory you authorized. Choose a cloud project when you want browser access, an isolated cloud environment, and cloud background execution.

### Can I use my own model service?

Yes. Okra supports OpenAI-compatible model services and lets you select different models for general chat, project management, environment analysis, memory summaries, and task execution.

### Can I intervene while the AI is working?

Yes. You can review tool activity, respond to confirmation requests, send additional guidance, stop the current run, and retry after a failure.

### Will a task continue after I close the page?

Cloud background tasks can continue running. Tasks for a local project depend on the desktop client on the current device, so its local runtime must remain online.

### Can cloud and local projects switch automatically?

No. They have different execution locations and data boundaries. Okra clearly indicates where the current project must run and does not switch silently.

## Current Product Status

Okra is evolving quickly. Current limitations include:

- Local projects require the desktop client.
- Local projects do not yet support chat attachments or image/file attachments in additional guidance sent while a task is running.
- Conversation data for cloud and local projects remains separate and is not migrated automatically.
- Available desktop platforms, versions, and registration rules depend on the Okra deployment you use.

## Technical and Self-Hosting Reference

The following section is for maintainers who deploy, debug, or extend Okra. Regular users do not need it.

<details>
<summary>Expand architecture notes and development commands</summary>

### Execution architecture

Okra has two isolated execution planes:

- Cloud Project: Chat OS Backend, Project Management, Task Runner, Memory Engine, Harness, and Sandbox Manager.
- Local Connector Project: Local Connector Core in the desktop client. Project, conversation, task, memory, and event data is primarily stored in local SQLite.
- The two planes share control-plane information such as users, agents, model catalogs, Plugin/Skill policies, and system configuration.
- Neither plane silently falls back to the other. Invalid entry points return `local_runtime_required` or an equivalent local runtime error.

### Local data paths

The default Local Connector state paths are:

```text
~/.chatos/local_connector/state.json
~/.chatos/local_connector/runtime.sqlite3
```

Set `LOCAL_CONNECTOR_STATE_PATH` to change the state file location. The SQLite database is stored in the same directory.

### Start the self-hosted cloud stack

Docker Engine and Docker Compose v2 are required:

```bash
cp docker/.env.example docker/.env
make docker-up
```

The main application is available at <http://localhost:8088> by default. The development administrator is `admin / admin123456`; production deployments must change the default password and internal secrets in `docker/.env`.

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

The complete local-project experience depends on the trusted Runtime Bridge provided by Electron. Core/settings development mode alone is not equivalent to the complete desktop client.

Package for macOS:

```bash
./local_connector_client/package-electron-macos-client.sh
```

Package for Windows:

```powershell
powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\package-electron-windows-client.ps1
```

### Build and test

```bash
make build
make smoke
make test
```

The main execution planes can also be tested independently:

```bash
cargo test -p chat_app_server_rs
cargo test -p task_runner_service_backend
cargo test -p local_connector_client_core
cd memory_engine/backend && cargo test
```

### Architecture sources of truth

- Deployment boundaries and ports: `docker/compose.yml`
- Rust workspace: `Cargo.toml`
- Cloud/local frontend routing: `chatos/frontend/src/lib/api/client/facades/`
- Cloud execution boundary: `chatos/backend/src/core/project_execution.rs`
- Local execution plane: `local_connector_client/core/src/local_runtime/`
- Local SQLite schema: `local_connector_client/core/migrations/`
- Cloud Task Runner: `task_runner_service/backend/src/services/`
- Project runtime environments: `project_management_service/backend/src/services/runtime_environment.rs`
- Development and deployment commands: `Makefile`, `docker/deploy.sh`, and `scripts/local-dev-stack.sh`
- `chatos_3d_anime_prototype/` is an experimental interface, not the current production entry point.

</details>

## License

This project is licensed under the [PolyForm Noncommercial License 1.0.0](./LICENSE). See [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md) for third-party notices.
