---
name: web-design-scene-building
description: Build or revise one bounded editable Scene Candidate with semantic nodes and compact tree operations.
metadata:
  chatos.role: leaf
---

# Build one Scene Step

Read `web_design_get_active_context.artboardDirectory` and choose exactly one relevant `artboardId` yourself. Work only on that semantic artboard and the Step returned by `deliveryGate.requiredNextAction`; `web_design_query_scene` and `web_design_edit_scene` reject cross-artboard operations. Do not ask the human to switch the directory for you.

Call `web_design_execute_step` with visible editable changes in `operationsJson`, a JSON-string encoding of one operation array. Keeping the operation contract here instead of repeating a large nested tool schema saves context without weakening server validation. The program supplies current-revision screenshots and grounding, chooses initial/retry/repair mode, and creates attempt, idempotency, and transaction IDs.

Prefer `insert-simple-tree` for a hierarchy:

```json
[{"op":"insert-simple-tree","parentId":"root:page","index":0,"tree":{"node":{"id":"hero","type":"frame","name":"Hero","frame":{"x":0,"y":0,"width":1440,"height":720}},"children":[{"node":{"id":"hero-title","type":"text","name":"Hero title","frame":{"x":96,"y":120,"width":720,"height":144},"content":"A real headline"}}]}}]
```

Serialize that complete array as the `operationsJson` string. Supported focused operations are `insert-simple-tree`, `insert-simple-node`, `insert-node`, `update-node`, `remove-node`, `move-node`, `insert-variable-collection`, and `insert-responsive-rule`. For an `update-node`, patches use path arrays such as `{"path":["appearance","opacity"],"value":0.8}`.

Every tree item uses `{node, children?}`. Only `frame` and `group` contain children. Descendants are stored as independent stable nodes and remain editable. Use `insert-simple-node`, update, move, or remove operations for focused changes. Raw nodes are for advanced Scene fields only.

Use semantic IDs and names, container hierarchy, Auto Layout or Grid, and one semantic value per text node. Do not encode navigation, cards, forms, tables, or whole pages in newline-heavy text. Do not flatten the result into an image.

For a focused post-generation adjustment, call `web_design_edit_scene` with the chosen `artboardId` and a JSON-string `commandJson`. Common commands are `move`, `resize`, `align`, `distribute`, `reorder`, `group`, `frame`, `auto-layout-frame`, `ungroup`, `delete-nodes`, `set-responsive-override`, `clear-responsive-override`, `add-annotation`, and `update-node`. Query the target stable IDs first. A command may touch only the chosen artboard.

For retry or repair, pass the failed `stepId` and change the visible structure or styling implicated by its issue IDs. Never replay an accepted Step. After execution, stop mutating and visually review the returned Candidate.
