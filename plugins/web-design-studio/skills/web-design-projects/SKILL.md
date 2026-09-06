---
name: web-design-projects
description: Discover, reuse, create, and revise Web Design Studio projects, documents, pages, annotations, and queued requests without confusing internal project IDs with ChatOS scope.
metadata:
  chatos.role: leaf
---

# Projects and documents

1. Call `web_design_list_projects` and reuse the intended internal project when it exists.
2. Create an internal project only when a distinct organizational container is needed.
3. Call `web_design_list_documents` before creating a document that may already exist.
4. Read the complete document, pages, component tree, annotations, requests, tokens, and revision before editing.
5. Preserve stable IDs and use `web_design_apply_patch` for focused changes.
6. On revision conflict, reread and rebase only the intended patch.

A website project may contain multiple designs; a design may contain multiple pages. Keep component `pageId` and parent relationships inside one page. Treat open annotations and page/component design requests as requirements, and resolve a request only after its change is applied.

Never copy the host-injected `scope.chatosProjectId` into a `projectId` argument. Read [project examples](references/examples.md) before create-versus-reuse decisions.
