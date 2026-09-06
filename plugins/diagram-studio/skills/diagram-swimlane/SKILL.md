---
name: diagram-swimlane
description: Design one Diagram Studio swimlane collaboration scenario with real ownership lanes, explicit handoffs, bounded activities, decisions, evidence, and positive and negative examples.
metadata:
  chatos.role: leaf
---

# Swimlane diagram generation

A swimlane diagram explains who owns each step in one collaboration scenario. Lanes are responsibility boundaries, not decorative rows.

## Scope and lanes

- Choose one outcome involving two or more roles, teams, systems, or organizations.
- Use 2–6 lanes. Combine roles whose responsibilities are indistinguishable in this scenario.
- Do not create one lane per activity, endpoint, class, or employee.
- Do not mix an organization's unrelated sales, procurement, HR, finance, and operations processes on one canvas.

## Activities and handoffs

- Put each activity in the lane accountable for performing it.
- Cross-lane edges represent a request, approval, notification, transfer, or responsibility handoff.
- Label important handoffs when the transferred object or decision is not obvious.
- Keep decisions in the lane of the role or system making them.
- Make terminal responsibility clear: who completes, rejects, or escalates the work?

## Complexity and layout

Keep no more than 24 activities and 28 edges. Prefer horizontal progression across aligned lanes. Avoid frequent zig-zagging caused by overly narrow roles. Extract repeated or optional subprocesses when they dominate the scenario.

## Evidence and PlantUML

For system lanes, inspect API ownership, event consumers, state transitions, and integrations. Human ownership may come from requirements; mark assumptions instead of inventing organizational rules. Use supported activity partitions/swimlanes, concise lane names, action labels, and named decision outcomes. Keep architecture and deployment elements out of the activity path.

Read [positive and negative swimlane examples](references/examples.md) before planning.

## Checklist

- `single_collaboration_scenario`: one trigger and outcome.
- `lanes_represent_real_ownership`: every lane is a meaningful responsibility boundary.
- `handoffs_are_explicit`: cross-lane transfers are readable.
- `decisions_have_named_outcomes`: decision ownership and outcomes are visible.
- `independent_scenarios_are_split`: unrelated collaborations are separate diagrams.
- `code_evidence_is_mapped`: system-derived activities have evidence.

Never use one swimlane diagram to contain all business processes. Create one diagram per collaboration scenario.
