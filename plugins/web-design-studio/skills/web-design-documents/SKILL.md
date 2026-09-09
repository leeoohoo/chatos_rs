---
name: web-design-documents
description: Discover, reuse, create, and revise Web Design Studio documents, pages, annotations, and queued requests inside the immutable scope injected by ChatOS.
metadata:
  chatos.role: leaf
---

# Documents and pages

1. Call `web_design_list_documents` before every create-versus-reuse decision.
2. Reuse an existing document when its title or content represents the same product, business flow, requested version, or design direction. A retry must continue the existing document.
3. Create a document only when the user explicitly asks for a separate design deliverable and no matching document exists.
4. Treat “第二版”, “重新设计”, “另一个方案”, and similar wording as a document or page decision inside the current injected scope.
5. Read `web_design_get_active_context` and the current Plan first, then query the affected Scene page or bounded node with `web_design_query_scene`. Preserve stable IDs and use revision-checked Candidate transactions.
6. On revision conflict, discard stale visual artifacts, reread the current Scene revision, recapture, and rebuild only the intended Step.

A design may contain multiple independent Scene artboards. A destination page, modal, Drawer, menu, overlay, or important interface state should have its own artboard when it needs design work; linking them does not merge their content. A planned artboard may remain intentionally incomplete while AI advances it across bounded Steps. Keep every Scene node inside one page tree. Treat open node annotations as requirements, and resolve one only after its visible change is applied and reviewed from a new screenshot.

ChatOS binds the active scope inside the runtime. Use only the declared document and page arguments. Read [document examples](references/examples.md) before create-versus-reuse decisions.
