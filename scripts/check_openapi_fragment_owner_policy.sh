#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bash "$ROOT_DIR/scripts/report_openapi_fragment_owners.sh" "$@" --strict
