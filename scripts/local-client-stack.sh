#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

STACK_PROFILE="local-client"
STACK_COMMAND="$0"

# shellcheck source=local-dev-stack/runner.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/local-dev-stack/runner.sh"
run_local_stack "$@"
