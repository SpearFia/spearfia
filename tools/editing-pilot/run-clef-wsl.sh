#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
set -eu
export HF_HOME="$HOME/clef/cache/huggingface"
export HF_MODULES_CACHE="$HOME/clef/cache/modules"
if [ -n "${SPEARFIA_CLEF_PID_FILE:-}" ]; then
    printf '%s\n' "$$" > "$SPEARFIA_CLEF_PID_FILE"
fi
exec "$HOME/clef/venv/bin/python" "$(dirname "$0")/clef_service.py"
