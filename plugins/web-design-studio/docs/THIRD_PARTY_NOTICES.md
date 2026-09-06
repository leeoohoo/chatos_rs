# Third-party component notices

Web Design Studio keeps each component system in an independent catalog and records the upstream source and license in both this file and the MCP catalog output.

## Ant Design

- Source: https://github.com/ant-design/ant-design
- License: MIT
- Included scope: all 72 components in Ant Design 6.6.2 and 828 independently runnable public documentation demos. Debug, semantic test harnesses, internal theme-token demos, and examples that depend on unpublished website internals are excluded from the product catalog.
- License text: [third-party-licenses/Ant-Design-MIT.txt](third-party-licenses/Ant-Design-MIT.txt)

The synchronization step copies the upstream TSX demo source without recreating its markup or interaction. Every demo is mounted lazily through the shared React registry adapter, so component variant counts follow the official source instead of a fixed handcrafted quota.

## Chakra UI

- Source: https://github.com/chakra-ui/chakra-ui
- License: MIT
- Included scope: 113 visual components from Chakra UI 3.37.0 and all 1,152 independently runnable official composition examples associated with them.
- License text: [third-party-licenses/Chakra-UI-MIT.txt](third-party-licenses/Chakra-UI-MIT.txt)

The synchronization step copies the official `apps/compositions/src/examples`, `ui`, and `lib` sources and mounts them through the shared React registry adapter with the official Chakra provider. `EnvironmentProvider` is intentionally absent because upstream publishes no visual documentation example for it; the product does not substitute a handwritten renderer or preserve a legacy fallback.

## Magic UI

- Source: https://github.com/magicuidesign/magicui
- License: MIT
- Included scope: the 68 `registry:ui` components whose source is currently downloadable from the official open-source repository, including all 123 currently downloadable official demo compositions across 65 components and three direct-source fallbacks, when audited on 2026-09-05.
- License text: [third-party-licenses/Magic-UI-MIT.txt](third-party-licenses/Magic-UI-MIT.txt)

Magic UI Pro templates and paid material are explicitly excluded. The Magic UI Pro license prohibits redistributing, reselling, sharing, or transferring that material and prohibits using it to create a competing template or component product.

The upstream root registry currently lists seven additional names whose source files and public registry endpoints return 404 (`script-copy-btn`, `flip-text`, `scratch-to-reveal`, `box-reveal`, `iphone-15-pro`, `arc-timeline`, and `grid-beams`). They are intentionally not presented as integrated components until the upstream project publishes the corresponding source.

## Spell UI

- Source: https://github.com/xxtomm/spell-ui
- License: MIT
- Included scope: all 33 files in the public `registry/spell-ui` component registry audited on 2026-09-04, mounted from official source through the shared React registry runtime.
- License text: [third-party-licenses/Spell-UI-MIT.txt](third-party-licenses/Spell-UI-MIT.txt)

## Inspira UI

- Source: https://github.com/rahulv-official/inspira-ui
- License: MIT
- Included scope: all 155 component documentation entries and all 197 official Vue demo files across the 12 public English documentation categories, synchronized from upstream commit `db389077d286742eaa1b35f965009f2984ff130b`.
- License text: [third-party-licenses/Inspira-UI-MIT.txt](third-party-licenses/Inspira-UI-MIT.txt)

All catalog variants are generated directly from the official `app/components/inspira/examples` tree and mounted through one shared Vue registry adapter. Components with multiple upstream demos expose every demo; components with one upstream demo accurately show one variant. Registry components and examples are copied without handwritten visual substitutes or compatibility patches. The one asset omitted from the GitHub Globe registry payload, `globe.json`, is copied from the same upstream commit as instructed by the official source. No paid or separately licensed material is included.

## shadcn/ui

- Source: https://github.com/shadcn-ui/ui
- License: MIT
- Included scope: all 61 `registry:ui` primitives in the official `new-york-v4` registry audited on 2026-09-04, plus 300 resolvable official examples and blocks.
- License text: [third-party-licenses/shadcn-ui-MIT.txt](third-party-licenses/shadcn-ui-MIT.txt)

The synchronization step validates local and aliased imports and excludes upstream previews that refer to unpublished internal files. Six primitives that currently publish no standalone official example (`Direction`, `Attachment`, `Bubble`, `Marker`, `Message`, and `MessageScroller`) are rendered with 14 declarative compositions made only from their official exported primitives and documented variant props. Those compositions use the same generic registry composition engine; they do not reimplement the components' visual CSS or interaction runtime.

## daisyUI

- Source: https://github.com/saadeghi/daisyui
- License: MIT
- Included scope: all 68 component documentation routes and all 587 official HTML examples in daisyUI 5.7.28, rendered with the official daisyUI CSS through a shared DOM registry adapter. Example payloads are split by component and loaded on demand; their upstream HTML structure and native interactions are preserved.
- License text: [third-party-licenses/daisyUI-MIT.txt](third-party-licenses/daisyUI-MIT.txt)

## React Bits exclusion

React Bits Pro is not included because its license prohibits redistribution and re-exposure as a component or block library. The public React Bits repository is also not included: its `MIT + Commons Clause License Condition v1.0` permits use in an application, website, or product but prohibits selling, sublicensing, or redistributing the components themselves, including bundled or ported versions. An editable component-library product falls inside that restriction.
