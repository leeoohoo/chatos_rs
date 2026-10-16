---
name: chatos-user-clarification
description: Collect a blocking Human choice, structured fields, or both through the run-bound Ask User tools. Use when proceeding would require a material assumption; do not use for casual conversation or information the user already supplied.
---

# Structured Human clarification

Ask only for information that materially changes the next action or deliverable. Continue the original task after the response instead of merely restating it.

When this Skill is available, a missing Human choice is a waiting-for-user state, not a terminal Task blocker. Ask as soon as the decision becomes necessary, preserve completed work, and resume the same Run from the answer. Do not emit a blocked outcome first, and do not ask about tool validation errors or technical failures that can be corrected autonomously.

Choose the smallest matching form:

- Use `prompt_choices` for a bounded single- or multi-choice decision.
- Use `prompt_key_values` for structured fields without a separate decision.
- Use `prompt_mixed_form` when the same clarification needs both fields and a choice.

Do not ask again when the answer is already present. Do not replace a clear user request with a lower-scope interpretation merely because a required fact is missing; ask for that fact. A form does not grant permission beyond the user's request.

Read [references/forms-secrets-and-recovery.md](references/forms-secrets-and-recovery.md) when collecting credentials or other sensitive values, confirming a high-impact action, or recovering from authentication, connection, or permission failures.
