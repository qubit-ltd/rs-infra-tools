#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
project_root=${RS_INFRA_PROJECT_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
shared_root=${RS_INFRA_SHARED_ROOT:-$(cd "$script_dir/../.." && pwd -P)}
source "$shared_root/.infra/lib/cleanup-build-artifacts.sh"
source "$shared_root/.infra/lib/network-retry.sh"
config="$project_root/.infra/ci/local-path-dependencies.tsv"
[[ "$#" -le 1 ]] || { echo "usage: $0 [--check-locked]" >&2; exit 2; }
mode=${1:-prepare}
[[ "$mode" == prepare || "$mode" == --check-locked ]] || {
    echo "usage: $0 [--check-locked]" >&2; exit 2;
}
if [[ ! -f "$config" ]]; then
    [[ "$mode" != --check-locked ]] && exit 0
    echo "error: locked dependency list is missing: $config" >&2
    exit 1
fi

# Pick an exact-destination, no-clobber rename implementation. BSD mv has no
# equivalent of GNU -T, so do not use its directory-merge behavior as fallback.
publish_mv=
for candidate in mv gmv; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" --help 2>&1 | grep -q -- ' -T'; then
        publish_mv=$candidate
        break
    fi
done
publish_python=
if [[ -z "$publish_mv" && "$(uname -s)" == Darwin ]]; then
    candidate_python=${RS_INFRA_PYTHON:-python3}
    if command -v "$candidate_python" >/dev/null 2>&1; then
        publish_python=$candidate_python
    fi
fi

is_checkout_root() {
    local candidate_root git_root
    [[ -d "$1" && ( -d "$1/.git" || -f "$1/.git" ) ]] || return 1
    candidate_root=$(cd "$1" 2>/dev/null && pwd -P) || return 1
    git_root=$(git -C "$candidate_root" rev-parse --show-toplevel 2>/dev/null) || return 1
    git_root=$(cd "$git_root" 2>/dev/null && pwd -P) || return 1
    [[ "$candidate_root" == "$git_root" ]]
}

while IFS=$'\t' read -r relative_path repository_url revision; do
    [[ -z "$relative_path" || "$relative_path" == \#* ]] && continue
    revision=${revision%$'\r'}
    [[ "$relative_path" == ../* && "$relative_path" != *$'\t'* ]] || {
        echo "error: invalid local dependency path '$relative_path'" >&2; exit 1;
    }
    [[ -n "$repository_url" && -n "$revision" ]] || {
        echo "error: incomplete local dependency entry '$relative_path'" >&2; exit 1;
    }
    target="$project_root/$relative_path"
    if [[ "$mode" == --check-locked ]]; then
        if [[ ! -e "$target" ]]; then
            echo "error: locked dependency '$relative_path' is missing (expected $revision)" >&2
            exit 1
        fi
        if ! is_checkout_root "$target"; then
            echo "error: locked dependency '$relative_path' is not a Git checkout root (expected $revision)" >&2
            exit 1
        fi
        actual_revision=$(git -C "$target" rev-parse HEAD 2>&1) || {
            echo "error: cannot read HEAD for '$relative_path': $actual_revision" >&2
            exit 1
        }
        if [[ "$actual_revision" != "$revision" ]]; then
            echo "error: locked dependency '$relative_path' HEAD mismatch: actual $actual_revision; expected $revision" >&2
            exit 1
        fi
        if actual_status=$(git -C "$target" status --porcelain 2>&1); then
            :
        else
            status_exit=$?
            echo "error: cannot read working-tree status for '$relative_path' (exit $status_exit): $actual_status" >&2
            exit 1
        fi
        if [[ -n "$actual_status" ]]; then
            echo "error: locked dependency '$relative_path' is dirty at $actual_revision; expected clean checkout at $revision" >&2
            printf 'actual status:\n%s\n' "$actual_status" >&2
            exit 1
        fi
        continue
    fi
    if is_checkout_root "$target"; then
        continue
    elif [[ -e "$target" || -L "$target" ]]; then
        echo "error: refusing to replace existing dependency path '$relative_path'" >&2
        exit 1
    fi

    [[ -n "$publish_mv" || -n "$publish_python" ]] || {
        echo "error: preparing missing dependency '$relative_path' requires GNU mv with -T -n or gmv, or Darwin renamex_np with Python 3" >&2
        exit 1
    }

    parent=$(dirname "$target")
    name=$(basename "$target")
    mkdir -p "$parent"
    staging=$(mktemp -d "$parent/.${name}.prepare.XXXXXX")
    lock="$parent/.${name}.prepare-publish.lock"
    lock_owned=false
    cleanup() {
        [[ -n "${staging:-}" ]] && rm -rf "$staging"
        if [[ "${lock_owned:-false}" == true ]]; then
            rmdir "$lock"
        fi
    }
    trap cleanup EXIT

    checkout="$staging/checkout-${staging##*.}"
    if [[ "$revision" =~ ^[[:xdigit:]]{40}$ ]]; then
        git init "$checkout"
        git -C "$checkout" remote add origin "$repository_url"
        infra_run_with_retry "fetch $repository_url revision $revision" \
            git -c http.connectTimeout="${RS_INFRA_GIT_CONNECT_TIMEOUT_SECONDS:-30}" \
            -C "$checkout" fetch --depth 1 origin "$revision"
        git -C "$checkout" checkout --detach FETCH_HEAD
    else
        infra_run_with_retry "clone $repository_url branch $revision" \
            git -c http.connectTimeout="${RS_INFRA_GIT_CONNECT_TIMEOUT_SECONDS:-30}" \
            clone --depth 1 --branch "$revision" "$repository_url" "$checkout"
    fi

    actual_revision=$(git -C "$checkout" rev-parse HEAD 2>&1) || {
        echo "error: cannot verify staged dependency '$relative_path': $actual_revision" >&2
        exit 1
    }
    if [[ "$revision" =~ ^[[:xdigit:]]{40}$ && "$actual_revision" != "$revision" ]]; then
        echo "error: staged dependency '$relative_path' HEAD mismatch: actual $actual_revision; expected $revision" >&2
        exit 1
    fi
    if actual_status=$(git -C "$checkout" status --porcelain 2>&1); then
        :
    else
        status_exit=$?
        echo "error: cannot verify staged dependency '$relative_path' status (exit $status_exit): $actual_status" >&2
        exit 1
    fi
    if [[ -n "$actual_status" ]]; then
        echo "error: staged dependency '$relative_path' is dirty: $actual_status" >&2
        exit 1
    fi

    if ! mkdir "$lock"; then
        echo "error: another dependency preparation is publishing '$relative_path'" >&2
        exit 1
    fi
    lock_owned=true
    if [[ -e "$target" || -L "$target" ]]; then
        echo "error: refusing to replace existing dependency path '$relative_path'" >&2
        exit 1
    fi
    move_status=0
    if [[ -n "$publish_mv" ]]; then
        if "$publish_mv" -Tn "$checkout" "$target"; then
            :
        else
            move_status=$?
        fi
    else
        if "$publish_python" -c 'import ctypes, errno, os, sys
source, destination = sys.argv[1:]
renamex_np = ctypes.CDLL(None, use_errno=True).renamex_np
renamex_np.argtypes = (ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint)
renamex_np.restype = ctypes.c_int
result = renamex_np(os.fsencode(source), os.fsencode(destination), 0x00000004)
if result != 0:
    error = ctypes.get_errno()
    raise OSError(error, os.strerror(error), destination)' "$checkout" "$target"; then
            :
        else
            move_status=$?
        fi
    fi
    if [[ -e "$checkout" || -L "$checkout" ]]; then
        if [[ -e "$target" || -L "$target" ]]; then
            echo "error: dependency path '$relative_path' appeared during publication; preserving it" >&2
        elif (( move_status != 0 )); then
            publisher=mv
            [[ -n "$publish_mv" ]] || publisher=Darwin\ renamex_np
            echo "error: failed to publish dependency path '$relative_path' ($publisher exit $move_status)" >&2
        else
            echo "error: failed to publish dependency path '$relative_path'" >&2
        fi
        exit 1
    fi
    if (( move_status != 0 )); then
        publisher=mv
        [[ -n "$publish_mv" ]] || publisher=Darwin\ renamex_np
        echo "error: failed to publish dependency path '$relative_path' ($publisher exit $move_status)" >&2
        exit 1
    fi

    rm -rf "$staging"
    staging=
    rmdir "$lock"
    lock_owned=false
    trap - EXIT
done < "$config"
