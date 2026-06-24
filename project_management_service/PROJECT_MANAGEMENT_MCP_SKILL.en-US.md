---
name: project-management-mcp-agent-en-us
description: English guide for AI agents using Project Management MCP to manage project base information, background, introduction, requirements, project work items, dependencies, and requirement technical overview documents.
---

# Project Management MCP Agent Skill

Project Management MCP is the structured project-management entry point exposed by the Project Management service. It manages project data, requirements, project work items, dependency relationships, and technical overview documents.

## Core Rules

- Treat `project_task` as a Project Management work item, also called `ProjectWorkItem`.
- Before creating a requirement or project work item, list or inspect existing records first. Update matching existing records instead of creating duplicates.
- Dependency tools use full replacement semantics. Read existing dependencies first to avoid removing user-maintained prerequisite relationships.
- Requirements can depend on prerequisite requirements. Project work items under a requirement can depend on prerequisite project work items.
- A requirement technical overview is managed by `upsert_requirement_technical_overview`; use it for implementation approach, architecture notes, API design, data structures, risks, and other overall technical content.
- Before creating a project work item, ensure the requirement has non-empty technical overview content. If it is empty, call `upsert_requirement_technical_overview` first, then call `create_project_task`.
- When the project description, background, or introduction is empty, too thin, or out of date with the current requirements, proactively maintain those project documents. Prefer evidence from user-provided information, project name, root path, Git URL, existing requirements, existing project work items, requirement technical overviews, and any visible README/docs/configuration context. If the content is reliable, call `initialize_project` to fill it in. If it cannot be inferred reliably, ask the user a few focused questions first. Do not invent facts.
- Project background, project introduction, and requirement technical overviews are long-form Markdown documents. Prefer clear headings, lists, key constraints, scope boundaries, and risks instead of one-line slogans.

## Tools

- `get_project_overview`: Get project base information and one-to-one profile.
- `initialize_project`: Initialize or update project base fields, background, and introduction.
- `list_requirements`: List project requirements.
- `create_requirement`: Create a project requirement.
- `update_requirement`: Update a requirement and optionally replace prerequisite requirements.
- `set_requirement_dependencies`: Replace one requirement's prerequisite requirement list.
- `upsert_requirement_technical_overview`: Create or update a requirement implementation technical overview.
- `get_requirement_technical_overview`: Read a requirement implementation technical overview.
- `list_project_tasks`: List project-management work items.
- `create_project_task`: Create a project-management work item under a requirement; requires non-empty technical overview content on that requirement.
- `update_project_task`: Update a project-management work item and optionally replace prerequisite work items.
- `set_project_task_dependencies`: Replace one project work item's prerequisite work item list.
- `get_project_dependency_graph`: Get the project dependency graph across requirements and project work items.

## Recommended Workflow

1. Call `get_project_overview` to inspect existing information for the current project.
2. If project description, background, or introduction is missing, inspect available context first: existing project data, requirements, project work items, technical overview documents, and any visible repository docs or configuration. When reliable content can be summarized, call `initialize_project` to fill the missing fields.
3. If missing project information cannot be inferred from available evidence, ask the user a small number of focused questions before writing it.
4. Call `list_requirements` before creating requirements.
5. Use `create_requirement` for new requirements or `update_requirement` for existing ones.
6. Before creating project work items, read or maintain the requirement technical overview. If its content is empty, call `upsert_requirement_technical_overview` first.
7. Call `list_project_tasks` before creating project work items with `create_project_task`.
8. Use `set_requirement_dependencies` and `set_project_task_dependencies` to maintain prerequisite relationships.
9. Call `get_project_dependency_graph` to verify the resulting dependency graph.
