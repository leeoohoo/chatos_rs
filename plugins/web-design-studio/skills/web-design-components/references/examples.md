# Component examples

## Positive: build one branded header Step as nodes

For an architecture studio site, create a `section` named `Editorial header` with Flex row layout. Add independent child nodes for `Wordmark`, `Selected work`, `Studio`, `Journal`, and `Project inquiry`. Use native typography for the wordmark and fetch the exact navigation or Button contract only when it supports the chosen quiet editorial direction. Submit only this header region, capture it, judge its spacing and hierarchy, then pause or continue with the next planned visual Step.

This is good because every object remains selectable, editable, movable, annotatable, and addressable by AI.

## Negative: a page-shaped text box

Do not create one 1392 × 976 `text` node whose content is:

```text
ATELIER NORTH    WORK    STUDIO    JOURNAL    CONTACT
Buildings that make room for life
Selected projects ...
```

Whitespace does not create navigation, metric cards, controls, or a table. This is descriptive prose disguised as UI and must fail handoff validation.

## Positive: component contract serving art direction

Use a returned shadcn Card binding for one supporting testimonial only when its density, border treatment, and slots fit the established visual system. Place editable quote and attribution children in the declared slots, and use native geometry or media for the page-specific composition around it.

## Negative: invented component system

Invent a library component or slot name, place every child in a generic `content` slot, or mix Ant Design buttons with Chakra fields in the same form without an explicit design reason.

## Positive: reusable navigation

Create one Scene component-main navigation definition, use component-instance nodes where the same navigation is required, and keep page-specific content overrides explicit.

## Positive: recover from a rejected Step

If a large Hero Candidate is rejected or its arguments are truncated, query the current Hero Scene subtree again and recapture its image. Keep the same visual intention and split it into later Steps such as `Hero / editorial copy`, `Hero / primary media`, and `Hero / supporting detail`. Preserve stable IDs between retries and judge the rendered composition before advancing.

## Negative: fallback after failure

Never respond to a rejected Scene transaction by inserting an image screenshot or one multiline text node named `完整页面`. The desired output is still the same editable Scene tree; only the Step boundary must become smaller.
