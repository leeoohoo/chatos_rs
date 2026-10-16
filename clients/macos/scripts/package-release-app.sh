#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}

CHATOS_BUILD_CONFIGURATION=release CHATOS_VERIFY_LAUNCH_SMOKE=1 "$SCRIPT_DIR/package-app.sh"
