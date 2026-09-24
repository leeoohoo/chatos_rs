# Forms, secrets, and recovery

## Form design

- Offer two or three mutually exclusive choices when the decision is bounded; put the recommended choice first and explain its concrete tradeoff.
- Ask for only fields required for the next safe action. Use stable field names and short, unambiguous labels.
- Separate a decision from its supporting fields when mixing them would make the response hard to interpret.

## Sensitive values

Mark passwords, tokens, private-key passphrases, and comparable credentials as secret fields. Explain their purpose without placing an example secret in the form. Never echo returned secrets into messages, task objectives, logs, notes, summaries, or later tool arguments that do not require them.

## High-impact actions

Ask before continuing when the unresolved choice changes the target, environment, cost, authorization, production impact, deletion or overwrite scope, or exposure of private data. Confirmation must name the concrete action and target; a generic "continue?" is insufficient.

## Recovery examples

Good:

- Authentication failed and two configured environments are plausible: ask which environment to use and request only the missing secret field.
- A migration could overwrite existing data: present the preserve-and-merge and replace choices with their effects.

Bad:

- Asking for an account or connection identifier already bound by the platform.
- Requesting a secret in ordinary chat because a secret form is available.
- Treating public probing or an offline guess as completion when the user asked for a real authenticated operation.
