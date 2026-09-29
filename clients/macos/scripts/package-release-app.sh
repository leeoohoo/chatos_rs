#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}

CHATOS_BUILD_CONFIGURATION=release "$SCRIPT_DIR/package-app.sh"
