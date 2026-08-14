#!/usr/bin/env bash
# Validate all runtime data/scenarios and representative execution paths.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
binary=(cargo run --locked --quiet -p ba-cli --bin ba-strategy --)
data_dir="$repo_root/data"

cd "$repo_root"

declare -A frozen_v2_sha256=(
    ["data/rulesets/jp_2026_07_29_provisional_v2.json"]="0d25ca7b3ca75c29667866920f21eb5b75456e55d44db87e1557ae631f2af49b"
    ["data/rewards/jp_2026_07_29_campaign_v2.json"]="ec437c160e8e884608889b34ac2d49131e2f40979f65fc9a8310a9d91025923a"
    ["data/rewards/jp_2026_07_29_empty_v2.json"]="9ed74f018543904ba203d6d3541ebf13857d0b60fe547feddd1d8467a0a6b08c"
)
for relative_path in "${!frozen_v2_sha256[@]}"; do
    observed="$(sha256sum "$relative_path" | awk '{print $1}')"
    [[ "$observed" == "${frozen_v2_sha256[$relative_path]}" ]] || {
        printf 'error: frozen v2 data changed: %s\n' "$relative_path" >&2
        exit 1
    }
done

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
"${binary[@]}" --data-dir "$data_dir" analyze scenarios/golden/v3_three_target_exact_small.json --format json >/dev/null
"${binary[@]}" --data-dir "$data_dir" analyze scenarios/golden/v3_four_target_exact_small.json --format json >/dev/null
"${binary[@]}" --data-dir "$data_dir" simulate scenarios/golden/v3_atomic_cross_target.json --runs 100 --seed 42 --format json >/dev/null
"${binary[@]}" --data-dir "$data_dir" compare scenarios/golden/v3_three_target_exact_small.json --runs 100 --seed 42 --format json >/dev/null

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
