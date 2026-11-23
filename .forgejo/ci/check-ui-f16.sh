#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if rg -n --glob '*.rs' --glob '*.wgsl' 'enable f16|SHADER_F16' \
    "$ROOT/Crates/ui" "$ROOT/Crates/ui_text" "$ROOT/Crates/ui_render"; then
    echo "The v1.7 UI must not require or request SHADER_F16." >&2
    exit 1
fi

echo "UI does not reference SHADER_F16."
