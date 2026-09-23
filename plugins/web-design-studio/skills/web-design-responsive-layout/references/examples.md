# Responsive examples

## Positive

The same product-page artboard uses a two-column hero at a wide required viewport. At an intermediate width it reduces the media column. At a narrow required width it changes the container to a vertical flow, makes the CTA full-width, and preserves readable line length.

## Negative

Create separate device-named artboards, copy wide-view coordinates into the narrow view, or scale every component down. This duplicates the wrong design unit and creates tiny text, overflow, and brittle absolute positioning.

## Repair

Inspect the affected semantic artboard at the failing viewport width, fix the smallest responsible container or constraint, rerun auto layout, and validate again instead of moving every descendant independently.
