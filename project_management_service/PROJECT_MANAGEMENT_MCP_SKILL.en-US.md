---
name: project-management-mcp-agent-en-us
description: English guide for AI agents using Project Management MCP to manage project base information, background, introduction, requirements, project work items, dependencies, and requirement technical overview documents, and to ensure every actionable requirement has project work-item coverage during planning or replanning.
---

# Project Management MCP Agent Skill

Project Management MCP is the structured project-management entry point exposed by the Project Management service. It manages project data, requirements, project work items, dependency relationships, and technical overview documents.

## Core Rules

- Treat `project_task` as a Project Management work item, also called `ProjectWorkItem`.
- Minimal effective modeling: first decide whether a new requirement layer is needed at all. If one requirement with multiple project work items expresses the work, do not split it into parent and child requirements. Requirements describe what must be delivered; project work items describe how to execute it.
- Before creating a requirement or project work item, list or inspect existing records first. Update matching existing records instead of creating duplicates.
- Requirement coverage invariant: every newly created or currently updated actionable requirement must have at least one corresponding project work item. Do not create work items only for the first requirement; if one planning pass creates N actionable requirements, the final state must show work-item coverage for all N requirements.
- Dependency tools use full replacement semantics. Read existing dependencies first to avoid removing user-maintained prerequisite relationships.
- Requirements can have parent-child hierarchy and prerequisite requirements. Project work items under a requirement can depend on prerequisite project work items. Prefer prerequisite relationships and project work items by default; use parent-child requirement hierarchy only when a child requirement needs its own scope, acceptance criteria, status, or dependencies.
- A requirement technical overview is managed by `upsert_requirement_technical_overview`; use it for implementation approach, architecture notes, API design, data structures, risks, and other overall technical content.
- Before creating a project work item, ensure the requirement has non-empty technical overview content. If it is empty, call `upsert_requirement_technical_overview` first, then call `create_project_task`.
- Do not create invalid groupings: if a parent requirement's "child requirements" are only execution steps, module splits, or a task list, create multiple `project_task` records directly under the parent requirement instead. For example, "parent requirement A + 3 child requirements + only 1 child has a work item" is invalid; use "requirement A + 3 project work items" and express order with project work-item prerequisites if needed.
- Create child requirements only when each child is an independently deliverable scope. Every actionable child requirement still needs project work-item coverage. If a parent requirement is only a summary, milestone, or documentation-only item and cannot be executed directly, explicitly record the "no direct project work item" reason in its detail, acceptance criteria, or technical overview, and ensure all actionable child requirements have project work-item coverage.
- When the project description, background, or introduction is empty, too thin, or out of date with the current requirements, proactively maintain those project documents. Prefer evidence from user-provided information, project name, root path, Git URL, existing requirements, existing project work items, requirement technical overviews, and any visible README/docs/configuration context. If the content is reliable, call `initialize_project` to fill it in. If it cannot be inferred reliably, ask the user a few focused questions first. Do not invent facts.
- Project background, project introduction, and requirement technical overviews are long-form Markdown documents. Prefer clear headings, lists, key constraints, scope boundaries, and risks instead of one-line slogans.

## Modeling Ladder

When adding or replanning project-management content, stop at the first layer that works:

1. An existing equivalent requirement or project work item covers it: update that record instead of creating a new one.
2. Multiple project work items under one requirement cover it: create or update `project_task` records, not child requirements.
3. Independent acceptance criteria, status, prerequisite dependencies, or delivery scope are needed: only then create child requirements.
4. Execution order is needed: prefer prerequisite requirements or prerequisite project work items; do not use parent-child hierarchy to simulate ordering.

## Requirement-To-Work-Item Coverage

During planning, replanning, requirement decomposition, or project-management backfill, keep a coverage matrix in scratch:

- Requirement: id, title, type, and status.
- Modeling decision: whether this is an independently deliverable requirement or should only be a project work item under a parent requirement.
- Technical overview: whether non-empty content exists; write it if missing.
- Project work items: existing or newly created work-item titles under that requirement.
- Exception reason: only summary, milestone, or documentation-only requirements may have no direct project work item, and the reason must be explicit; all actionable child requirements must have work-item coverage.

Before finishing, use `list_project_tasks` and `get_project_dependency_graph` to verify the coverage matrix. If any actionable requirement has no project work item, create the missing work item or update a matching existing one before ending the run.
If the current run created "one parent requirement + multiple title-only or step-like child requirements", collapse that shape into project work items under the parent requirement first; do not finish after attaching a project work item to only one child.

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
5. Before creating a requirement, run the Modeling Ladder: when the work can be expressed as project work items under an existing requirement, create or update `project_task` records instead of child requirements.
6. Only when independent delivery scope is truly needed, use `create_requirement` for new requirements or `update_requirement` for existing ones.
7. For every newly created or currently updated actionable requirement, read or maintain the requirement technical overview. If its content is empty, call `upsert_requirement_technical_overview` first.
8. For every actionable requirement, call `list_project_tasks` to check existing coverage. If no project work item covers it, call `create_project_task` to create at least one actionable project work item.
9. Use `set_requirement_dependencies` and `set_project_task_dependencies` to maintain prerequisite relationships.
10. Call `get_project_dependency_graph` to verify the resulting dependency graph and confirm that every actionable requirement appears with a corresponding project work item; if the graph contains task-like child requirements, convert them into project work items under the parent requirement before ending the run.
