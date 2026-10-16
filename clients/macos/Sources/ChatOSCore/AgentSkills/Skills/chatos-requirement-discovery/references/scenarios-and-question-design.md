# Scenarios and question design

Start with what is already known, identify decisions that would change the deliverable, then choose only applicable questions. Ask in the user's vocabulary, explain consequential tradeoffs, and distinguish needs from suggested solutions. Avoid technical choices the user need not make. Offer a sensible recommendation without silently selecting it for them.

## Software and product work

For “build a financial system” or “improve this app,” understand who will use it, the core workflow and pain point, expected platform/integrations, data and security boundaries, MVP exclusions and evidence of success. Finance might need clarification of fictional prototype versus real bookkeeping, account/role isolation, approval responsibilities and required report periods. Do not ask for database or framework preferences unless the choice is genuinely constrained or important to the user. A precise bug diagnosis or a specified test run usually needs no survey.

## Design and creative work

Clarify the intended audience, use context, brand/message, aesthetic references and dislikes, accessibility constraints, target formats and whether editable source or export is required. Separate exploring options from implementing an already approved design. Do not dispatch competing writers against one design source; collect read-only proposals or serialize the final edit.

## Writing, marketing and localization

Understand the audience and desired response, publication channel, tone, factual/source requirements, length/format, language/locale and sensitive claims. Ask for examples only when they materially help. Do not invent product claims or treat a draft request as permission to publish. For a clearly specified rewrite, proceed without a generic audience survey.

## Research, data and operational decisions

Clarify the decision the research supports, scope and time period, trusted sources, evaluation criteria and preferred output. For spreadsheet/report work ask about metric definitions, units, reporting period and editable deliverable where unknown. Distinguish live production data/actions from isolated validation. Tool/network failure is a recovery problem, not a reason to re-ask user preferences.

## Mixed artifacts and staged work

Confirm shared goals before splitting development, design and copy. Identify which choices gate others, who owns each source of truth, and what the user will accept. Convert answers into concrete expected outputs and acceptance criteria rather than copying the entire questionnaire into every Task. Keep independent work independent after decisions are known.

## Question patterns

- Outcome: “What should users be able to accomplish first?” rather than “Which features do you want?”
- Priority: offer realistic tradeoffs (speed to prototype, visual polish, production readiness), not leading good/bad options.
- Boundary: separate must-have, later and explicitly excluded work; do not convert optional wishes into mandatory scope.
- Open preference: a bounded choice may include “not decided; recommend a plan,” with optional free text for another preference.
- Acceptance: ask which observable result would count as success, not whether the user is “satisfied.”

After submission, synthesize only the decisions that affect the next plan. If the user says “you decide,” choose and state proportionate assumptions within existing authority; do not repeat the same question until they choose a preferred implementation.
