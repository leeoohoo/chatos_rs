# Project Switch Performance Optimization Plan

## Goal

Make project/session switching cache-first and realtime-first.

## Principles

- Persist project-local metadata under `.chatos/`.
- Use realtime push for change propagation.
- Avoid automatic re-analysis on tab switches.
- Keep settings and git data cached unless the user explicitly refreshes.

## Current Direction

- Files: cache directory listings and incrementally invalidate them on changes.
- Code nav: persist symbol indexes and only rebuild dirty files.
- Run settings: read cached catalog/environment first; refresh only on demand or real project-root changes.
- Git: hydrate cached summary/details on open; force refresh only when needed.
- Terminals: restart must wait for old process exit before relaunching.

## Remaining Risk

- If a realtime payload is missing, the UI should fall back to cached state, not eager refetch.
- Long-lived terminal state may still need one final cleanup pass for edge cases after restart/stop.

