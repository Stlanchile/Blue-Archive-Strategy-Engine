#!/usr/bin/env bash
# Run cargo-audit in fresh-online or cached advisory-database mode.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
mode="${1:-online}"

if [[ $# -gt 1 ]]; then
    printf 'usage: %s [online|--no-fetch]\n' "${0##*/}" >&2
    exit 2
fi

if ! command -v cargo-audit >/dev/null 2>&1; then
    printf '%s\n' 'error: cargo-audit is required; install it with: cargo install cargo-audit --locked' >&2
    exit 1
fi

cd "$repo_root"
case "$mode" in
    online)
        cargo audit --deny warnings
        ;;
    --no-fetch)
        cargo audit --deny warnings --no-fetch
        ;;
    *)
        printf 'usage: %s [online|--no-fetch]\n' "${0##*/}" >&2
        exit 2
        ;;
esac
