---
name: diagram-sequence
description: Design one precise Diagram Studio runtime sequence scenario with causal messages, bounded activation bars and fragments, complexity limits, evidence, and PlantUML sequence guidance.
metadata:
  chatos.role: leaf
---

# Sequence diagram generation

A sequence diagram explains one runtime scenario in time order. It is not a swimlane process, a static architecture map, or a place to combine every user journey in the system.

## Scope and participants

Use the `runtime-scenario` mode for one trigger and one causally connected outcome, such as login token refresh, order submission, task dispatch, webhook delivery, or one failure/retry scenario.

- Use no more than 8 participants and 20 messages.
- Participants must have distinct runtime roles. Merge internal objects that add no meaningful interaction boundary.
- Split login, ordering, payment, fulfillment, refund, and reporting into separate diagrams even when they belong to the same product.
- If success and failure paths share the same trigger, a small `alt` fragment may keep them together. Independent triggers or long subflows require separate diagrams.

## Messages

- Put messages in causal top-to-bottom order.
- Use solid request arrows for synchronous calls and dashed return arrows for responses.
- Use an asynchronous arrow or a dashed semantic relationship for events, queues, callbacks, or fire-and-forget delivery when supported by the source.
- Label messages with intent, not implementation noise: `提交订单`, `校验令牌`, `发布任务`, `返回结果`.
- Do not invent replies for asynchronous messages or omit a response that determines the next visible action.

## Activation bars

- A synchronous incoming call starts or extends the receiver's activation interval when no suitable activation already covers that message time.
- Do not create a second overlapping activation merely because another message targets an already-active participant.
- End an activation after its response or when the participant's shown work for this scenario ends.
- Nested synchronous work may create a nested activation only when it represents real nested execution.
- Activation bars are narrow execution intervals. Their width is fixed; only their vertical start and height should vary.
- Do not place message labels, arrowheads, or participant headers behind activation bars. Messages should terminate at the visible activation edge when an activation exists.

## Fragments

- Use `alt` for mutually exclusive outcomes, `opt` for optional behavior, and `loop` only for a bounded repeated condition.
- Give every operand a concise condition.
- Size fragments around their messages with padding; the frame and header must not cover labels, arrows, activation bars, or participant headers.
- Do not use one giant loop or alt frame as a background for most of the diagram. Split a long branch or repeated subprocess instead.

## Evidence and PlantUML

Inspect request handlers, clients, interfaces, events, callbacks, queue consumers, state transitions, retries, and error paths. Map participant aliases to `sourceReferences`. Do not infer message order from folder names alone.

Use PlantUML participants such as `actor`, `participant`, `boundary`, `control`, `entity`, `database`, `queue`, and concise unique ASCII aliases. Use `activate`/`deactivate` only where the interval is meaningful; the Diagram Studio importer may also derive a receiver activation from synchronous calls and must avoid duplicates. Use `alt`/`else`/`end`, `opt`, and bounded `loop` fragments.

Read [positive and negative sequence examples](references/examples.md) before submitting the generation plan.

## Checklist

- `single_runtime_scenario`: one trigger and causally connected outcome.
- `participants_have_distinct_roles`: every participant represents a meaningful runtime boundary.
- `message_order_is_causal`: requests, asynchronous messages, and returns are ordered and typed correctly.
- `activation_intervals_are_bounded`: activations start and end correctly without duplicate overlaps.
- `fragments_do_not_hide_content`: alt, opt, and loop frames contain but do not obscure their messages.
- `independent_scenarios_are_split`: unrelated user journeys or business outcomes are separate diagrams.

Do not place every product workflow into one sequence diagram. A useful diagram set has one named sequence per runtime scenario and uses an architecture overview to connect the larger system context.
