# Agent draft quality

## Responsibility gap

Good: compare the requested work with existing member responsibilities and define a distinct, reusable role.

Bad: create a duplicate Agent because an existing member is busy, or encode one short-lived task as the permanent identity.

## Role prompt

Describe objectives, responsibilities, decision boundaries, collaboration expectations, and evidence standards. Do not grant permissions, invent tools, or include hidden IDs.

## Model and profession

Use only values returned by `model_list` and `profession_list`. Choose the profession that best matches ongoing work, not merely a keyword in the brief. Use a supported thinking level or accept the listed default.

## Confirmation

After `agent_draft`, report a pending draft. The Human confirmation path revalidates catalogs; if an option became unavailable, refresh and produce a corrected draft instead of forcing the stale choice.
