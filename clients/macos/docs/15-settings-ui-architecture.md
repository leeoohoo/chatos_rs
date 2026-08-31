# Settings UI Architecture

## Goal

Every page in the ChatOS Settings window should follow the visual language of the original **General** page: a macOS grouped-settings surface with compact section headers, rounded grouped rows, explanatory footer text, and no oversized page hero.

The same data views are also used by the standalone Native Connector control center. Styling the Settings window must not silently change that surface or alter the semantic container used by dynamic controls.

## Incident and root cause

The shared `LocalConnectorCard` was changed globally from a neutral visual container into a SwiftUI `Section`. That component is used by model and plugin pages containing dynamic `TextField`, `Grid`, and rapidly changing connector state. The global semantic change caused SwiftUI AttributeGraph to enter a continuous platform text-field update loop, keeping the main thread near 100% CPU and making ChatOS unresponsive.

The failure was not a backend or connector deadlock. It was a view-graph/layout loop caused by applying a page-level design decision inside a shared low-level component.

## Required structure

- `SettingsView` owns navigation, page selection, banners, and the navigation title.
- `SettingsGroupedPage` owns the Settings-only page surface and spacing.
- `LocalConnectorCard` remains container-neutral and never becomes a `Form`, `List`, or `Section`.
- `LocalConnectorCardPresentation.settingsGrouped` changes appearance only. It does not change hierarchy or control semantics.
- Standalone connector pages keep `LocalConnectorCardPresentation.card`.
- Complex model/plugin controls remain in stable `ScrollView`/`VStack` layouts rather than being forced into `Form` sizing.

## Change rules

1. A visual change must identify whether the target is the Settings window, the standalone control center, or both.
2. Do not change the semantic container of a shared component to achieve page styling.
3. New Settings pages use `SettingsGroupedPage` and existing neutral content components.
4. Headers and footer descriptions belong to the grouped presentation; business controls remain reusable.
5. Pet overlay changes must not share state or layout components with Settings pages.

## Verification gate

Before considering a Settings UI change complete:

1. Build the full Swift package.
2. Launch the packaged app, not only the SwiftPM executable.
3. Open every Settings destination: General, Pet, AI Models, Device & Gateway, Plugins & Skills, Command Approvals, Runtime & Permissions, and Access Control.
4. Confirm page switching remains responsive and the ChatOS process does not sustain high CPU.
5. Exercise at least one editable model/plugin field and one toggle/picker page.
6. Verify the standalone Native Connector control center still uses its intended card presentation.
7. Re-run the full test suite.

Any sustained main-thread CPU spike, AttributeGraph loop, or Computer Use timeout during page switching is a release blocker, even when compilation and unit tests pass.
