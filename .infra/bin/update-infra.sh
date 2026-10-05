#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
[ "$#" -eq 0 ] || { echo "usage: update-infra.sh" >&2; exit 2; }
[ -x "${RS_INFRA_TOOLS_BIN:-}" ] || { echo "error: rs-infra-tools runtime is unavailable" >&2; exit 1; }
exec "$RS_INFRA_TOOLS_BIN" prewarm --project "$project_root"
