# Responsive examples

## Positive

Desktop uses a two-column hero. Tablet reduces the media width. Mobile changes the container to a vertical flow, makes the CTA full-width, and preserves readable line length.

## Negative

Copy desktop coordinates to mobile or scale every component down. This creates tiny text, overflow, and brittle absolute positioning.

## Repair

Inspect the affected page and device, fix the smallest responsible container or constraint, rerun auto layout, and validate again instead of moving every descendant independently.
