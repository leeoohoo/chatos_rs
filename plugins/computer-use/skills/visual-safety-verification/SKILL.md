---
name: visual-safety-verification
description: Verify visible outcomes and apply authorization boundaries before consequential Visual Computer Use actions on the user's real desktop.
metadata:
  chatos.role: leaf
---

# Safety and verification

Computer-use events affect the user's real desktop. Inspection or navigation does not authorize a materially different mutation.

Before sending or publishing content, purchase, deletion, permission change, credential submission, account change, installation, or another hard-to-reverse action:

1. Observe the exact target and foreground application.
2. Confirm the intended effect, recipient, scope, and irreversible details visible on screen.
3. Obtain any confirmation required by the host or user.
4. Perform one bounded action.
5. Verify the resulting visible state or use a more authoritative read-only check.

Do not expose screenshot contents, credentials, tokens, personal data, or clipboard data beyond the task. If the visible UI cannot support a reliable next action, stop with the last confirmed state and the smallest needed user intervention.

Read [verification examples](references/examples.md) before consequential actions.
