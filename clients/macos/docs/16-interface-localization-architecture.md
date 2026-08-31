# Interface localization architecture

ChatOS has two independent language preferences:

- **Interface language** controls client-owned labels, buttons, statuses, empty states, validation messages, alerts, and pet overlays.
- **Internal context language** controls subsequent AI system prompts, built-in MCP prompts, and task-process context. It must not change existing user or server content.

## UI copy rules

1. Static SwiftUI literals use Chinese as the source key. Every key must have an English entry in `Support/Localization/en.lproj/Localizable.strings` and an identity entry in `Support/Localization/zh-Hans.lproj/Localizable.strings`. The Chinese table prevents Bundle from falling back to English while the interface locale is Chinese.
2. Dynamic client-owned copy uses `AppModel.localized(_:english:)` or a language-aware display mapper.
3. Project names, usernames, paths, commands, model output, user messages, and server-returned document bodies are displayed verbatim.
4. Domain DTOs keep machine state. Views map state codes to localized display text instead of persisting translated strings.
5. Every independently hosted SwiftUI surface, including the global pet overlay, receives the current interface locale.

## Verification

Run `scripts/audit-interface-localization.sh` before packaging. It verifies both English coverage and the Simplified Chinese identity table. Then switch the interface language in the running app and smoke-test the main window, Settings, sheets, alerts, task details, terminal, notepad, remote connections, Git, and the pet overlay in both directions.
