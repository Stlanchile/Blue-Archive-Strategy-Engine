#!/usr/bin/env bash
# Validate all runtime data/scenarios and representative execution paths.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
binary=(cargo run --locked --quiet -p ba-cli --bin ba-strategy --)
data_dir="$repo_root/data"

cd "$repo_root"

for document in "$data_dir"/rulesets/*.json "$data_dir"/rewards/*.json; do
    [[ -f "$document" ]] || {
        printf 'error: expected shipped document is missing: %s\n' "$document" >&2
        exit 1
    }
    "${binary[@]}" --data-dir "$data_dir" validate "$document" --format json >/dev/null
done

for scenario in scenarios/golden/*.json scenarios/examples/*.json; do
    [[ -f "$scenario" ]] || {
        printf 'error: expected shipped scenario is missing: %s\n' "$scenario" >&2
        exit 1
    }
    "${binary[@]}" --data-dir "$data_dir" validate "$scenario" --format json >/dev/null
done

"${binary[@]}" --data-dir "$data_dir" analyze scenarios/golden/single_target_200.json --format json >/dev/null
"${binary[@]}" --data-dir "$data_dir" simulate scenarios/golden/ticket_atomic.json --runs 1 --seed 42 --trace --format json >/dev/null
"${binary[@]}" --data-dir "$data_dir" compare scenarios/golden/single_target_200.json --runs 100 --seed 42 --format json >/dev/null

stdout_file="$(mktemp)"
stderr_file="$(mktemp)"
cleanup() {
    rm -f -- "$stdout_file" "$stderr_file"
}
trap cleanup EXIT

if "${binary[@]}" --data-dir "$data_dir" validate "$repo_root/does-not-exist.json" --format json >"$stdout_file" 2>"$stderr_file"; then
    printf '%s\n' 'error: induced validation failure unexpectedly succeeded' >&2
    exit 1
fi
if [[ -s "$stdout_file" ]]; then
    printf '%s\n' 'error: induced validation failure wrote to stdout' >&2
    exit 1
fi
