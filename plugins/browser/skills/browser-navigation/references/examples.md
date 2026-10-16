# Navigation examples

## Positive: search and inspect

Goal: find the installation section of a repository.

1. Open the bound session and read its mode.
2. Navigate to the search page.
3. Snapshot, open the matching result with a current ref, then snapshot the repository page.
4. Locate and inspect the installation section on the destination page.
5. Report only text verified on that page.

## Negative: snippet substitution

Navigate to search results, copy the snippet, and report that the repository says the same thing. This is wrong because snippets may be stale, truncated, or from another section.

## Positive: official article link

The official homepage snapshot contains `MiniMax H3` with a reference and an observed `/blog/minimax-h3` href. Describe `browser_click`, click the reference, wait if loading, then snapshot the destination and read its article text and publication date. The observed href is evidence; the title alone is not.

## Negative: title-to-slug fallback

Construct `/news/minimax-h3-an-open-model-breaking-the-boundaries-between-tasks-and-modalities` from the displayed title, receive 404, and invent another date-shaped media URL. These are guessed URLs even if they look plausible. Return to the verified homepage and click the actual link.

## Positive: timeout recovery

When navigation times out, call status and snapshot. If the expected destination is visible, continue. If not, retry once.

## Negative: duplicate navigation loop

Repeatedly call navigate after every timeout without checking state. This can create duplicate work and hide a page that already loaded.
