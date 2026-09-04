---
name: diagram-flowchart
description: Design one bounded Diagram Studio business or technical flowchart with explicit decisions, terminal outcomes, failure paths, complexity limits, and PlantUML activity guidance.
metadata:
  chatos.role: leaf
---

# Flowchart generation

A flowchart explains how one outcome is reached. It should not become a catalog of every workflow in the product.

## Scope

Choose one business outcome or subprocess, such as submitting an order, approving a refund, reserving inventory, or recovering a failed task. State the trigger and terminal outcomes before adding intermediate steps.

Use a swimlane diagram when ownership across roles is central. Use a sequence diagram when message timing between systems is central.

## Complexity

- Keep the flow at or below 24 meaningful activities and 28 edges.
- Prefer one start trigger and a small set of clearly different terminal outcomes.
- Split unrelated starts, products, or independent business goals.
- Extract a complex repeated subprocess instead of expanding it everywhere.
- Do not add technical calls, database operations, and UI gestures unless they change the business decision being explained.

## Activities and decisions

- Activity labels are actions: “校验订单”, “预占库存”, “记录失败原因”.
- Decision labels are questions: “库存充足？” or “审批通过？”.
- Every branch has an outcome label such as `是/否`, `通过/拒绝`, or a named status.
- Failure causes may share one handler only when their treatment is identical.
- A retry loop shows its exit condition and cannot appear infinite.

## Layout and evidence

Default to top-to-bottom reading. Keep the happy path visually dominant, put exceptional exits to one side, and rejoin only when semantics converge. For code-derived flows, inspect handlers, state transitions, validation branches, events, and error returns. Map aliases to source references and do not invent transitions.

## PlantUML

Use `start`, activities, `if/then/else/endif`, bounded loops, and `stop` or `end`. Keep one decision per question and use short branch labels. Do not use architecture packages and components to simulate a flowchart.

Read [positive and negative flowchart examples](references/examples.md) before planning.

## Checklist

- `single_business_outcome`: one outcome or bounded subprocess.
- `start_and_terminal_states_are_clear`: trigger and exits are visible.
- `decisions_have_named_outcomes`: all decision branches are labeled.
- `failure_and_retry_paths_are_bounded`: exceptions and loops terminate.
- `independent_processes_are_split`: unrelated workflows are separate diagrams.
- `code_evidence_is_mapped`: code-derived activities have evidence.

Never put every business process into one flowchart. Create several named flowcharts in the same transmitted scope.
