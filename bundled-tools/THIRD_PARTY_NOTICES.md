# Third-Party Notices

## ripgrep (`rg`)

- Project: https://github.com/BurntSushi/ripgrep
- License: MIT OR Unlicense

Bundled binaries under `bundled-tools/*/rg` are redistributed as command-line
tool binaries for local project search. When refreshing binaries from upstream
release archives, keep the upstream license files with the release artifact or
update this notice accordingly.

## agent-browser

- Project: https://github.com/vercel-labs/agent-browser
- License: Apache-2.0
- Packaged version: 0.31.2

The desktop packaging scripts obtain the exact npm release and stage only the
native executable for the target platform together with its license.

## Chrome for Testing

- Project: https://googlechromelabs.github.io/chrome-for-testing/
- Packaged version: 150.0.7871.115

Chrome for Testing is staged as the browser engine used exclusively by the
local `agent-browser` runtime. Its upstream notices remain inside the browser
archive.
