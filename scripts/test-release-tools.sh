#!/usr/bin/env bash
# Fast, side-effect-free checks for release helper syntax and policy invariants.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
scripts=(
    audit.sh check-release-workflow.sh check-workflow-pins.sh package-release.sh
    test-release-tools.sh verify-release-archive.sh verify-shipped-data.sh
)
for script in "${scripts[@]}"; do
    bash -n "$repo_root/scripts/$script"
done

"$repo_root/scripts/check-workflow-pins.sh"
"$repo_root/scripts/check-release-workflow.sh"
"$repo_root/scripts/package-release.sh" --help >/dev/null 2>&1
"$repo_root/scripts/verify-release-archive.sh" --help >/dev/null 2>&1
