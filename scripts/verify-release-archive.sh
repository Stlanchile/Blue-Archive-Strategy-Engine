#!/usr/bin/env bash
# Verify a packaged Linux runtime archive and smoke-test it outside the repository.
set -euo pipefail

usage() {
    printf 'usage: %s <archive.tar.gz>\n' "${0##*/}" >&2
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
    exit 0
fi
[[ $# -eq 1 ]] || { usage; exit 2; }
archive="$1"
[[ -f "$archive" ]] || { printf 'error: archive does not exist: %s\n' "$archive" >&2; exit 1; }
command -v tar >/dev/null 2>&1 || { printf '%s\n' 'error: tar is required' >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { printf '%s\n' 'error: sha256sum is required' >&2; exit 1; }

archive_dir="$(cd -- "$(dirname -- "$archive")" && pwd -P)"
archive_name="$(basename -- "$archive")"
checksum="$archive_dir/$archive_name.sha256"
if [[ -f "$checksum" ]]; then
    (cd "$archive_dir" && sha256sum --check "${checksum##*/}")
fi

mapfile -t entries < <(tar -tzf "$archive")
[[ "${#entries[@]}" -gt 0 ]] || { printf '%s\n' 'error: archive is empty' >&2; exit 1; }
root="${entries[0]%%/*}"
[[ "$root" == ba-strategy-v*-x86_64-unknown-linux-gnu ]] || {
    printf 'error: unexpected archive root: %s\n' "$root" >&2
    exit 1
}
for entry in "${entries[@]}"; do
    [[ "$entry" != /* && "$entry" != ../* && "$entry" != *"/../"* && "$entry" != ".." ]] || {
        printf 'error: unsafe archive entry: %s\n' "$entry" >&2
        exit 1
    }
    [[ "$entry" != "$root/tests/fixtures" && "$entry" != "$root/tests/fixtures/"* ]] || {
        printf 'error: test fixtures must not be included in runtime archive: %s\n' "$entry" >&2
        exit 1
    }
done

require_entry() {
    local path="$root/$1"
    local entry
    for entry in "${entries[@]}"; do
        if [[ "$entry" == "$path" ]]; then
            return
        fi
    done
    printf 'error: archive is missing %s\n' "$path" >&2
    exit 1
}

for path in \
    ba-strategy RELEASE-MANIFEST.sha256 README.md CHANGELOG.md SECURITY.md LICENSE-MIT LICENSE-APACHE \
    docs/CALIBRATION.md docs/PROTOCOLS.md docs/SCHEMA_V2.md docs/STRATEGIES.md docs/THREAT_MODEL.md \
    data/rulesets/jp_2026_07_29_provisional_v2.json \
    data/rewards/jp_2026_07_29_campaign_v2.json data/rewards/jp_2026_07_29_empty_v2.json; do
    require_entry "$path"
done

extract_root="$(mktemp -d)"
unrelated_dir="$(mktemp -d)"
cleanup() {
    rm -rf -- "$extract_root" "$unrelated_dir"
}
trap cleanup EXIT
tar -xzf "$archive" --directory "$extract_root"
(
    cd "$extract_root/$root"
    sha256sum --check RELEASE-MANIFEST.sha256
)

if [[ "$(uname -s)" == Linux ]]; then
    binary="$extract_root/$root/ba-strategy"
    [[ -x "$binary" ]] || { printf '%s\n' 'error: packaged binary is not executable' >&2; exit 1; }
    (
        cd "$unrelated_dir"
        "$binary" --data-dir "$extract_root/$root/data" catalog list all --format json >/dev/null
        "$binary" --data-dir "$extract_root/$root/data" --scenario-dir "$extract_root/$root/scenarios/examples" \
            analyze single_target_v2 --format json >/dev/null
    )
fi
