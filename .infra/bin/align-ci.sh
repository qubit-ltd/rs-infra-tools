#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
shared_root=${RS_INFRA_SHARED_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
source "$shared_root/.infra/lib/cleanup-build-artifacts.sh"
export RS_INFRA_STYLE_TOOLCHAIN="${RS_INFRA_STYLE_TOOLCHAIN:-nightly-2026-06-05}"
if [ -f "$project_root/.infra/style/rustfmt.toml" ]; then
    export RS_INFRA_STYLE_RUSTFMT_CONFIG_OVERRIDE="$project_root/.infra/style/rustfmt.toml"
elif [ -f "$project_root/rustfmt.toml" ]; then
    export RS_INFRA_STYLE_RUSTFMT_CONFIG_OVERRIDE="$project_root/rustfmt.toml"
fi
"$shared_root/.infra/bin/prepare-local-path-dependencies.sh"
"$shared_root/.infra/bin/infra-tool.sh" rs-infra-style --project "$project_root" fix "$@"
if [ "${RUN_COVERAGE_IN_ALIGN:-0}" = 1 ]; then
    "$shared_root/.infra/bin/coverage.sh"
fi
