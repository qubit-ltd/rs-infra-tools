#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
shared_root=${RS_INFRA_SHARED_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
source "$shared_root/.infra/lib/cleanup-build-artifacts.sh"
export CARGO_INCREMENTAL=1
# Rust 1.94 non-incremental coverage can lose counters for inline functions.
if [[ ${CARGO_ENCODED_RUSTFLAGS+x} ]]; then
    export CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS:+${CARGO_ENCODED_RUSTFLAGS}$'\x1f'}-Clink-dead-code"
else
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Clink-dead-code"
fi
"$shared_root/.infra/bin/prepare-local-path-dependencies.sh"
"$shared_root/.infra/bin/infra-tool.sh" rs-infra-coverage --project "$project_root" collect "$@"
"$shared_root/.infra/lib/coverage-report.sh"
"$shared_root/.infra/bin/infra-tool.sh" rs-infra-coverage --project "$project_root" check --input "$project_root/coverage.json"
