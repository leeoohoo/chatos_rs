#!/bin/zsh

set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
project_root="$(cd "${script_dir}/.." && pwd)"
app_path="${project_root}/dist/Visual Computer Use.app"
contents_path="${app_path}/Contents"
macos_path="${contents_path}/MacOS"
binary_name="visual-computer-use-mcp"

cd "${project_root}"
swift build -c release --product "${binary_name}"

rm -rf "${app_path}"
mkdir -p "${macos_path}"
cp ".build/release/${binary_name}" "${macos_path}/${binary_name}"
cp "Packaging/Info.plist" "${contents_path}/Info.plist"
chmod +x "${macos_path}/${binary_name}"

plutil -lint "${contents_path}/Info.plist" >/dev/null
codesign --force --deep --sign "-" \
    --requirements '=designated => identifier "com.visualcomputeruse.mcp"' \
    "${app_path}" >/dev/null

echo "Built: ${app_path}"
echo "Signed ad-hoc with designated requirement: identifier com.visualcomputeruse.mcp"
echo "MCP executable: ${macos_path}/${binary_name}"
echo "Permission guide: open \"${app_path}\" --args --onboarding"
