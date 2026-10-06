#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=$script_dir
while [[ ! -d "$project_root/.infra" && "$project_root" != / ]]; do
    project_root=$(dirname "$project_root")
done
if [[ ! -d "$project_root/.infra" ]]; then
    echo "error: unable to locate project .infra directory" >&2
    exit 2
fi

mode=${1:-}
if [[ "$mode" == --check ]]; then
    exec python3 "$project_root/.infra/bootstrap-check.py"
fi
if [[ "$mode" != "" && "$mode" != --yes && "$mode" != --dry-run && "$mode" != --status ]]; then
    echo "usage: ./update-infra.sh [--yes|--dry-run|--status|--check]" >&2
    exit 2
fi

tmp_root=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
work=$(mktemp -d "$tmp_root/rs-infra-bootstrap.XXXXXX")
cleanup() { rm -rf -- "$work"; }
trap cleanup EXIT
repository=https://github.com/qubit-ltd/rs-infra-tools.git
if ! git clone --quiet --depth 1 --single-branch --branch main "$repository" "$work/source"; then
    echo "error: unable to fetch the latest rs-infra-tools bootstrap package" >&2
    exit 1
fi
export RS_INFRA_SOURCE_REVISION
RS_INFRA_SOURCE_REVISION=$(git -C "$work/source" rev-parse HEAD)
python3 "$work/source/assets/project-bootstrap/sync.py" \
    --project-root "$project_root" \
    --package-root "$work/source/assets/project-bootstrap" \
    "$@"
