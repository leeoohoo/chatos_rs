---
name: chatos-requirement-discovery
description: Understand material project goals, audience, constraints, preferences and acceptance decisions through the durable Requirement Survey center before committing to a plan. Use in project Main Chat before Task creation, or for a newly discovered local Task blocker; not for clear requests, casual questions or secrets.
---

# Project requirement discovery

Help the user express the outcome they actually want, not just choose implementation details. Reuse facts and preferences already in the conversation. Ask when missing answers would materially change the scope, design, content, cost, risk, ownership or acceptance criteria; do not turn every request into a questionnaire.

For a project-bound Main Chat, activate this Skill and call `requirement_survey_create` directly before creating dependent Tasks. A survey is not a Task: do not create a placeholder worker merely to ask questions. Use the same existing Requirement Survey center as local Task surveys, not a separate storage or Agent-team workflow. A Task can use its authorized survey capability when a new blocker appears during execution.

When the program-owned `user_selected_capability_contract` says the user chose Requirement Survey in the composer, treat that as an explicit request for this workflow. Use the user's natural-language message to design the survey; never ask them for a tool name, Skill ref, version, schema, or internal identifier.

Read [references/scenarios-and-question-design.md](references/scenarios-and-question-design.md) when designing a survey for uncertain or multi-disciplinary work.

## Ask, wait, then use the answers

- Name the decision the survey enables in `title` and briefly explain it in `description` using the user's language.
- Ask a small coherent round of high-value questions, normally 3–6 rather than a fifty-question inventory. Use `text`, `single_choice`, `multiple_choice` or `boolean`; choice questions need at least two distinct options, while text/boolean have no options. Mark only genuinely blocking questions required. Allow uncertainty or another preference through an explicit choice plus optional text where useful.
- Call the survey tool alone, not in the same batch as Task creation or another mutation. The Host derives project, source conversation and Run; never supply IDs or pretend another project is bound.
- Creation suspends the source Run. Wait for the user's actual submission in the Requirement Survey center; do not call `wait_for_task_completion`, fabricate a final response, invent answers, or start work that depends on them. The UI provides the pending-survey notice.
- Submission resumes the same Run with structured answers. Resolve any real contradiction concisely and synthesize a concrete proposal: confirmed goals and audience, scope and exclusions, the recommended solution and important tradeoffs, staged deliverables/dependencies, acceptance evidence, and remaining decisions. Adapt depth to the request, not a fixed template. Do not just acknowledge the form, copy answers, or return only a task handoff. Respect explicit exceptions such as a test that asks for a one-sentence summary.
- Publish the proposal with `requirement_plan_save` using the returned survey ID, `expected_revision: 0` for its first draft, and the complete proposal in `markdown`. For a revision use the current revision from program-bound plan context, preserve supplied decisions, and explain material changes. Finish with a useful user-facing proposal and ask the user to review and confirm its exact version in Requirement Survey. Read [references/proposal-binding.md](references/proposal-binding.md) when publishing, revising or dispatching from a proposal.
- Confirmation is a human-only UI action, not a model tool. Never infer it from submitted answers, “you decide,” or a previous version's confirmation. Even an already requested implementation waits for this new proposal's confirmation before dependent Tasks. Confirmation alone does not authorize implementation, payments, publishing, deletion or broader access; preserve the user's original implementation scope and leave unanswered optional points open.
- Use another focused round only when the answer reveals a new material decision. Never ask again for supplied information or seek commitments beyond the user's scope. Survey submission is not approval for payments, publishing, deletion or broader access.

The current survey module requires a project-bound conversation. In a contact-only chat, ask a concise conversational clarification instead of inventing a project or claiming a survey was created. Do not put passwords, tokens, payment details or unnecessary personal data into persistent survey fields.
