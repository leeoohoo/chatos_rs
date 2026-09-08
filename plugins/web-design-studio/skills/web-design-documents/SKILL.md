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
5. Read the outline first, then the affected page or bounded node. Preserve stable IDs and use revision-checked focused mutations.
6. On revision conflict, reread and rebase only the intended patch.

A design may contain multiple pages. Keep component `pageId` and parent relationships inside one page. Treat open annotations and page/component design requests as requirements, and resolve a request only after its change is applied.

ChatOS binds the active scope inside the runtime. Use only the declared document and page arguments. Read [document examples](references/examples.md) before create-versus-reuse decisions.
