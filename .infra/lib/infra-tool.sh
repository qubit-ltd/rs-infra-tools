#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
shared_root=${RS_INFRA_SHARED_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
source "$shared_root/.infra/lib/cleanup-build-artifacts.sh"
[ "$#" -ge 1 ] || { echo "usage: infra-tool.sh rs-infra-TOOL [ARGS...]" >&2; exit 2; }
tool="$1"
shift
case "$tool" in
    rs-infra-ci|rs-infra-coverage|rs-infra-dependency|rs-infra-pages|rs-infra-style|rs-infra-verify) ;;
    *) echo "error: unknown infra tool '$tool'" >&2; exit 2 ;;
esac
if [ -n "${RS_INFRA_BIN_DIR:-}" ] && [ -x "$RS_INFRA_BIN_DIR/$tool" ]; then
    exec "$RS_INFRA_BIN_DIR/$tool" "$@"
fi
tool_runner=${RS_INFRA_TOOLS_BIN:-$project_root/.infra/tools/bin/rs-infra-tools}
[ -x "$tool_runner" ] || { echo "error: rs-infra-tools runtime is unavailable" >&2; exit 1; }
exec "$tool_runner" latest --project "$project_root" --tool "$tool" -- "$@"
