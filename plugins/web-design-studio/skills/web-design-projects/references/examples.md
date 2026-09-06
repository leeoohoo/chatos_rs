# Project examples

## Positive

List projects, find “Marketing Site”, list its documents, reuse “Pricing redesign”, read revision 12, patch the pricing section, and validate revision 13.

## Negative

Create a new project and document for every prompt, or pass the outer ChatOS project ID as the plugin's `projectId`. Both cause duplicate or mis-scoped work.

## Revision conflict

Reread the document, locate the same stable component IDs, and reapply only the requested delta. Do not replace the whole document with a stale copy.
