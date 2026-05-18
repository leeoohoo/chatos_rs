# System UI I18N Implementation Plan

## Why this is needed

The project already has `INTERNAL_CONTEXT_LOCALE`, but that setting only controls backend-generated internal prompt/context language.

It does **not** control frontend UI copy, so the current product can easily end up with:

1. Chinese page shells
2. English dialog labels
3. mixed tool-detail cards
4. Chinese runtime panels with English fallback states

That is exactly why the system currently feels inconsistent.

## Root cause

There is no real frontend UI locale layer today.

Current state:

1. most UI strings are hardcoded directly in React components
2. some old areas were written in English first
3. newer areas were added in Chinese
4. the existing locale-related work only covers backend internal context

So this is not a single-page bug. It is a system-level missing capability.

## Correct product model

The system should have **two separate language settings**:

1. `UI_LOCALE`
   - controls frontend interface text
   - affects buttons, dialogs, titles, status labels, empty states, error fallbacks

2. `INTERNAL_CONTEXT_LOCALE`
   - controls Chatos-authored backend internal context/prompt language
   - affects runtime context composition only

These two settings solve different problems and should not be conflated.

## Work completed in this round

This round establishes the first usable system-level UI i18n base:

1. backend user settings now support `UI_LOCALE`
2. frontend now has a lightweight `I18nProvider` and dictionary-based `useI18n`
3. settings panel now exposes both:
   - UI language
   - internal context language
4. the following core surfaces have started using shared UI translations:
   - app bootstrap/loading shell
   - auth panel
   - error boundary
   - runtime settings panel
   - header bar
   - message list key states
   - chat welcome shell
   - chat composer placeholder
   - applications modal title
   - remote verification modal
   - show process summary + modal chain

## What still needs migration

The system is now on the right architecture, but many existing modules still contain hardcoded strings and should be migrated in batches:

1. tool detail cards
2. MCP manager
3. AI model manager
4. agent manager
5. notepad
6. project explorer
7. runtime drawers and summary panes
8. remote transfer/SFTP panels
9. Mermaid/code utility popups
10. legacy shared components and fallback messages

## Recommended migration order

### Phase 1

High-frequency shared UI:

1. tool detail cards
2. tool renderer summaries
3. manager dialogs
4. remote panels

### Phase 2

Workspace and explorer UI:

1. project explorer
2. team member workspace
3. runtime drawers
4. summary panes

### Phase 3

Long tail cleanup:

1. utility overlays
2. empty/error/loading fallbacks
3. story/test copy alignment

## Guardrails

To avoid future mixed-language regressions:

1. new user-facing strings should go through `useI18n`
2. backend/internal context strings should continue using `INTERNAL_CONTEXT_LOCALE`
3. external content must remain untouched:
   - user input
   - tool output
   - browser/web extracted content
   - remote command output
   - persisted memory summaries

## Definition of done

System-level UI i18n is considered complete when:

1. switching `UI_LOCALE` updates all major frontend shells
2. no common user path shows mixed Chinese/English UI labels by default
3. `INTERNAL_CONTEXT_LOCALE` remains independent
4. external payload text remains in original language
