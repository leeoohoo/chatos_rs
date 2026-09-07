# Component examples

## Positive: build a dashboard header as nodes

Create a `section` named `Dashboard header` with Flex row layout. Add independent child nodes for `Product logo`, `Workspace switcher`, `Global search`, `Notifications button`, and `User avatar`. Fetch the exact Input/Button/Avatar contracts before binding production components. Submit only this header region, reread the page, then continue with the sidebar.

This is good because every object remains selectable, editable, movable, annotatable, and addressable by AI.

## Negative: a page-shaped text box

Do not create one 1392 × 976 `text` node whose content is:

```text
LOGO    Dashboard  Projects  Settings          Search       Avatar
Welcome back
Revenue $128,400     Users 12,842     Conversion 8.4%
Recent orders ...
```

Whitespace does not create navigation, metric cards, controls, or a table. This is descriptive prose disguised as UI and must fail handoff validation.

## Positive: component contract

Use a returned shadcn Card binding for pricing cards, place editable heading and price children in the declared content slot, and use native decorative shapes behind the section.

## Negative: invented component system

Invent a library component or slot name, place every child in a generic `content` slot, or mix Ant Design buttons with Chakra fields in the same form without an explicit design reason.

## Positive: reusable navigation

Create one navigation symbol, instantiate it on each page, keep content overrides local, and synchronize structural changes from the definition.

## Positive: recover from a rejected batch

If a complete dashboard mutation is rejected or its arguments are truncated, call `web_design_get_page` again. Build `App shell`, `Sidebar`, `Top bar`, `Metrics grid`, and `Recent activity` as separate batches. Preserve stable IDs between retries.

## Negative: fallback after failure

Never respond to a rejected component-tree call by inserting an image screenshot or one multiline text component named `完整页面`. The desired output is still the same editable component tree; only the mutation size must change.
