---
name: web-design-studio
description: Route editable website design work to project, component-system, responsive-layout, visual-system, and validation or export specialists in Web Design Studio.
metadata:
  chatos.role: router
  chatos.related-skills: "web-design-projects,web-design-components,web-design-responsive-layout,web-design-visual-system,web-design-validation-export"
---

# Web Design Studio

Use Web Design Studio when the user wants an editable website, landing page, interface composition, or focused visual revision. The primary output is a coherent human-editable design; code export is secondary and happens only when requested.

## Runtime scope

ChatOS injects the outer user/project/public scope. Never ask for, invent, or pass a ChatOS project ID. Tool `projectId` values are internal Web Design Studio project identifiers returned by the plugin.

## Route the work

- Activate `web-design-projects` for project/document discovery, reuse, revisions, pages, annotations, and request queues.
- Activate `web-design-components` before choosing library bindings, slots, nesting, symbols, interactions, or reusable components.
- Activate `web-design-responsive-layout` for frames, constraints, Flex/Grid, auto layout, desktop/tablet/mobile behavior, and overflow repair.
- Activate `web-design-visual-system` for tokens, typography, color, spacing, imagery, effects, states, and coherent art direction.
- Activate `web-design-validation-export` before completion, conflict recovery, validation, preview, or HTML/React/Vue export.

Use this router activation as `parent_activation_ref`. A substantial new page normally needs projects, components, responsive layout, visual system, and validation. A focused text edit may need only projects and validation.

## Platform Skill protocol

Activate this router through `skill_skill_activate`, activate every leaf that materially governs the requested edit using this activation as `parent_activation_ref`, and pass all returned evidence tokens in `skillEvidence`. Use the platform resource tools only for references declared by an activated Skill. Do not invent ChatOS scope fields or evidence.

## MCP tool directory

- Projects and documents: `web_design_list_projects`, `web_design_create_project`, `web_design_get_project`, `web_design_update_project`, `web_design_delete_project`, `web_design_list_documents`, `web_design_create_document`, `web_design_get_document`, and `web_design_move_document` manage the plugin's editable project structure inside the injected outer scope.
- Components and templates: `web_design_get_component_library` inspects available systems; `web_design_insert_section` adds a bounded section; `web_design_apply_page_template` replaces one page from a template; `web_design_replace_document` replaces a complete editable document when a full redesign is intended.
- Focused editing: `web_design_apply_patch` applies revision-checked operations; `web_design_auto_layout` computes bounded layout changes; `web_design_sync_symbol_instances` refreshes instances from definitions; `web_design_update_symbol_from_instance` promotes an instance change into its reusable symbol.
- Requests: `web_design_list_requests` reads user-authored visual requests; `web_design_resolve_request` marks one request resolved only after the corresponding edit exists.
- Quality and delivery: `web_design_validate` checks document invariants and device layouts; `web_design_export_html`, `web_design_export_react`, and `web_design_export_vue` produce secondary code artifacts after the editable design is ready.

## Invariants

- Read the current complete document and revision before modifying it.
- Preserve stable IDs and unrelated user-authored content.
- Use focused patches for focused requests.
- Treat templates and sections as editable starting material, not a design boundary.
- Validate desktop, tablet, and mobile for new or substantially redesigned pages.
