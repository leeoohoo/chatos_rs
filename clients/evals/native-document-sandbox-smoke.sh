#!/bin/bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
test_root=$(mktemp -d "${TMPDIR:-/tmp}/chatos-native-document-smoke.XXXXXXXX")
trap 'rm -rf -- "$test_root"' EXIT

# Build/test an unpacked package, never edit or register a real installation.
cd "$repo_root/plugins/document"
npm run build
npm run sbom:check
npm run audit:skill-gates
npm pack --ignore-scripts --json --pack-destination "$test_root" > "$test_root/pack.json"
package_name=$(node -e 'const fs=require("node:fs"); process.stdout.write(JSON.parse(fs.readFileSync(process.argv[1],"utf8"))[0].filename)' "$test_root/pack.json")
case "$package_name" in
  chatos-document-mcp-*.tgz) ;;
  *) echo 'Unexpected Document package name' >&2; exit 1 ;;
esac
tar -xzf "$test_root/$package_name" -C "$test_root"
cd "$repo_root/clients/macos"
CHATOS_DOCUMENT_TEST_PACKAGE_ROOT="$test_root/package" swift test --filter NativeDocumentPluginIntegrationTests
