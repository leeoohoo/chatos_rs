---
name: browser-observation-verification
description: Observe and verify Browser CDP state with fresh snapshots, screenshots, focused extraction, and evidence-based completion claims.
metadata:
  chatos.role: leaf
---

# Browser observation and verification

Choose the smallest authoritative read for the question.

- Use `browser_snapshot`'s top-level `url/title/ready_state/text/text_truncated` and `nodes` for real page identity, bounded visible text, roles, link `href`, and fresh refs. Empty nodes do not mean an empty or nonexistent page. If text is truncated, use focused observation or scrolling; never assume the omitted article content.
- Use screenshots when visual arrangement, charts, canvas, image content, or rendering defects matter.
- Use `browser_session_status` for session health, current mode, tabs, or recovery decisions.

Extract only information needed for the request. Re-snapshot after any action that may change content. When reporting completion, cite the observed state: destination, visible confirmation, changed value, downloaded artifact, or other concrete evidence.

Do not infer hidden state from styling, a spinner disappearing, or a click response. Do not expose unrelated cookies, credentials, personal data, or page content.

For dated evidence, use the host-provided current date/time and UTC offset, not the model's training date or a page's claims about today. Distinguish article publication from update dates and explicitly record missing dates. Insufficient independent evidence is an acceptance gap, not proof of browser execution failure.

Read [verification examples](references/examples.md) when deciding whether an action truly completed.
