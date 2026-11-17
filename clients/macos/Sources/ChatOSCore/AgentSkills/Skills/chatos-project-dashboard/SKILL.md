---
name: chatos-project-dashboard
description: Read and update the structured ChatOS project dashboard using current Todo evidence, milestones, risks, Human actions, health, and optimistic revision checks.
---

# Project dashboard

Read the dashboard before updating it. Treat program-generated Todo and run statistics as facts; do not overwrite them with narrative estimates.

Use the current revision, stable milestone and issue identifiers, and Todo references returned for the same team. Health, progress, and completion must follow observed task state, accepted deliverables, survey results, or Human messages.

Keep next steps actionable. Ordinary task blockers remain manager-owned and must not be copied into the Human action list. Record a Human-owned issue only when a real decision, credential, approval, budget, authority, or external action is required after the manager has tried the available coordination paths.

Write the dashboard for a non-technical project owner. Keep `summary` to two to four short sentences covering the current outcome, impact, material risk, and decision that deserves attention. Do not paste internal status values, IDs, commit hashes, file paths, database or DTO names, commands, test inventories, or evidence transcripts into `summary` or `next_steps`; keep those details in Todo progress, Runs, or shared assets. Use no more than five next steps, each phrased as one concrete action in the user's language.

Every Human-owned issue must say why only the Human can act, what the manager already tried, the impact and decision deadline, two or three concrete options when applicable, the manager's recommendation, and one precise requested action.

Read [references/revisions-and-evidence.md](references/revisions-and-evidence.md) for first creation, concurrent updates, milestone progress, health classification, issues, and completion claims.
