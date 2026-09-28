#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}

# Backward-compatible development entry point. Production and deployment scripts
# must use package-release-app.sh so a Debug binary cannot be installed by accident.
CHATOS_BUILD_CONFIGURATION=debug "$SCRIPT_DIR/package-app.sh"
