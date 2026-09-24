---
name: chatos-agent-builder
description: Build one Human-confirmed ChatOS Agent draft from a frozen project snapshot and current model and profession catalogs. Use only inside the dedicated Agent Builder run.
---

# Agent Builder

Inspect before drafting:

1. Read `project_inspect` to understand the project, team goal, and existing responsibilities.
2. Read `model_list` and `profession_list`; select only current returned values.
3. Draft one durable Agent identity that fills a real responsibility gap.
4. Submit exactly one `agent_draft` when the name, role, responsibility, role prompt, model, thinking level, profession, and rationale agree.

The draft is pending Human confirmation. It does not create an Agent, add a member, select Plugins, or modify the project.

Read [references/draft-quality.md](references/draft-quality.md) for overlap checks, role-prompt boundaries, model/profession selection, and rejection recovery.
