#!/usr/bin/env bash
# Verify that every GitHub Action is pinned to an immutable commit SHA.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
workflow_dir="$repo_root/.github/workflows"

if [[ ! -d "$workflow_dir" ]]; then
    printf 'error: workflow directory is missing: %s\n' "$workflow_dir" >&2
    exit 1
fi

found=0
while IFS= read -r -d '' workflow; do
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ "$line" =~ ^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*([^[:space:]#]+) ]] || continue
        found=1
        reference="${BASH_REMATCH[2]}"
        if [[ ! "$reference" =~ ^[^@[:space:]]+@[0-9a-f]{40}$ ]]; then
            printf 'error: action is not pinned to a full immutable SHA in %s: %s\n' \
                "${workflow#"$repo_root"/}" "$reference" >&2
            exit 1
        fi
        if [[ ! "$line" =~ \#[[:space:]]+v[0-9] ]]; then
            printf 'error: pinned action has no human-readable version comment in %s: %s\n' \
                "${workflow#"$repo_root"/}" "$reference" >&2
            exit 1
        fi
    done < "$workflow"
done < <(find "$workflow_dir" -type f \( -name '*.yml' -o -name '*.yaml' \) -print0 | sort -z)

if [[ "$found" -eq 0 ]]; then
    printf 'error: no GitHub Action references found under %s\n' "$workflow_dir" >&2
    exit 1
fi
