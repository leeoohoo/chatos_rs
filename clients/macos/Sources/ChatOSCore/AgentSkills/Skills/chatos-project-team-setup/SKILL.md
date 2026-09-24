---
name: chatos-project-team-setup
description: Propose a ChatOS project team for an existing project, a new managed project, or an explicitly supplied local directory. Use when the Human asks to create or attach a project team; proposals always require Human confirmation.
---

# Project team setup

Start with `project_catalog`. Use the returned project facts and current-run options; never guess a project identifier or filesystem path. Activate this Skill with the returned `skill_ref` before submitting a proposal.

Choose exactly one setup mode:

- Use `team_propose_existing` for an existing ChatOS project that the catalog says has no active team.
- Use `team_propose_new_project` when the Human explicitly wants a new ChatOS-managed project and team.
- Use `team_propose_import_directory` only when the Human supplied an existing local absolute directory. Preserve that path exactly.

A remote Git URL, `git@...` address, repository name, or imagined `/path` is not a local directory. Do not turn a request to clone, inspect, or run a repository into a project-team proposal; route that work through Todo execution and its approved capabilities.

Every write tool creates a pending proposal, not a project or team. State that Human confirmation is still required, then wait for the confirmation result before claiming anything exists.

Read [references/modes-and-failures.md](references/modes-and-failures.md) when choosing between modes, handling existing-team conflicts, importing a directory, or responding to confirmation and failure results.
