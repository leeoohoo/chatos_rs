---
name: visual-pointer-keyboard
description: Perform verified macOS mouse, scroll, text, and keyboard actions through Visual Computer Use while preserving focus and avoiding blind action chains.
metadata:
  chatos.role: leaf
---

# Pointer and keyboard interaction

For every pointer target: observe, locate, move, visually confirm the cyan hotspot, act, then verify the returned screenshot.

- `click` has no coordinates and acts at the last verified virtual-cursor position.
- Use one left click by default. Double, right, or repeated clicks require visible justification.
- Before typing, click the field and verify focus or an insertion point when visible.
- `type_text` is a real write action and does not prove which field received text.
- Use `key_press` for one key or chord. `capture_after: false` is only for deterministic intermediate keyboard steps after focus was just verified; the next state-changing step must produce a screenshot.
- Move the cursor over the intended scroll container. Use moderate 200–500 pixel deltas, verify direction and progress, and stop when the target appears or content no longer moves.

After a modal, menu, autocomplete list, alert, navigation, or app activation, discard old coordinates and observe again. If an action is ambiguous, widen the observation and reacquire once rather than repeatedly clicking.

Read [interaction examples](references/examples.md) for focus and recovery patterns.
