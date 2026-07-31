#!/usr/bin/env bash
# Prefer sccache when installed; otherwise invoke rustc directly.
# Cargo calls: rustc-wrapper <rustc> <args...>
set -euo pipefail
if command -v sccache >/dev/null 2>&1; then
  exec sccache "$@"
else
  exec "$@"
fi
