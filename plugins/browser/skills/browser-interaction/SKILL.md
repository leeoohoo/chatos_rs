---
name: browser-interaction
description: Interact with Browser CDP pages through fresh element refs, focused form operations, bounded scrolling, and post-action verification.
metadata:
  chatos.role: leaf
---

# Browser interaction

Use the latest accessibility snapshot as the interaction source of truth.

## Reliable loop

1. Snapshot after the latest page transition.
2. Select the exact current ref and verify its role, name, and surrounding context.
3. Use `browser_click`, `browser_type`, `browser_fill_form`, or `browser_scroll`.
4. Take a fresh snapshot and verify the expected state before continuing.

For forms, fill only fields required for the user's goal. Preserve existing values unless replacement is intended. Before submission, distinguish ordinary reversible navigation from consequential external actions that require confirmation.

If a ref is stale or absent, take one fresh snapshot and reacquire the target. Do not retry the old ref. Scroll in moderate increments and stop when the target is visible or successive snapshots show no movement.

Read [interaction examples](references/examples.md) when handling forms, dynamic menus, or stale refs.
