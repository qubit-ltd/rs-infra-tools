#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$script_dir}
shared_root=${RS_INFRA_SHARED_ROOT:-$script_dir}
exec "$shared_root/.infra/bin/update-infra.sh" "$@"
