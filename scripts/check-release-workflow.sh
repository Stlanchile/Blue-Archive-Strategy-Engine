#!/usr/bin/env bash
# Ensure release verification remains read-only and publication cannot run project code.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
workflow="$repo_root/.github/workflows/release.yml"

if [[ ! -f "$workflow" ]]; then
    printf 'error: release workflow is missing: %s\n' "$workflow" >&2
    exit 1
fi

"$repo_root/scripts/check-workflow-pins.sh"

require() {
    local expression="$1"
    local description="$2"
    if ! grep -Eq -- "$expression" "$workflow"; then
        printf 'error: release workflow is missing %s\n' "$description" >&2
        exit 1
    fi
}

require '^  release-verify:$' 'the release-verify job'
require '^  release-publish:$' 'the release-publish job'
require 'needs: release-verify' 'the verification dependency'
require 'environment: release' 'the protected release environment'
require 'contents: read' 'read-only verification permissions'
require 'contents: write' 'publication write permission'
require 'tags: \["v\*"\]' 'a tag-only release trigger'
require 'scripts/package-release.sh' 'verified artifact packaging'
require 'scripts/verify-release-archive.sh' 'archive verification'

publisher="$(sed -n '/^  release-publish:/,$p' "$workflow")"
if grep -Eq -- 'actions/checkout|cargo[[:space:]]|rustup|scripts/' <<<"$publisher"; then
    printf '%s\n' 'error: release-publish must not checkout, build, run Cargo, or execute project scripts' >&2
    exit 1
fi
if ! grep -Eq -- 'actions/download-artifact@[0-9a-f]{40}' <<<"$publisher"; then
    printf '%s\n' 'error: release-publish must download only verified artifacts' >&2
    exit 1
fi
