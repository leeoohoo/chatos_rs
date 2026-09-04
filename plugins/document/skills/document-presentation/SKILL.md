---
name: document-presentation
description: Create and revise PPTX artifacts with coherent slide structure, concise text, deliberate ordering, and mandatory slide rendering for visual verification.
metadata:
  chatos.role: leaf
---

# Presentations

Inspect an existing PPTX before editing and render the slides that will be changed. Use `office_create` with `format: "pptx"` for new decks and `office_edit_batch` for copied revisions.

Each slide should have one communication purpose. Keep titles concise, body text scannable, and slide order causal. Use typed slide, textbox, text-update, reorder/delete, and slide-property operations. Do not place an entire report into one slide or duplicate the same dense layout across every slide.

After editing, validate and render every changed slide. Inspect for clipping, overflow, weak hierarchy, inconsistent backgrounds, and accidental hidden slides. Return the managed PPTX artifact; export to PDF only when requested and disclose that the current Office-to-PDF path is image-based preview fidelity.

Read [presentation examples](references/examples.md) before a new deck or structural revision.
