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
command -v find >/dev/null 2>&1 || { printf '%s\n' 'error: find is required' >&2; exit 1; }
command -v sort >/dev/null 2>&1 || { printf '%s\n' 'error: sort is required' >&2; exit 1; }
command -v xargs >/dev/null 2>&1 || { printf '%s\n' 'error: xargs is required' >&2; exit 1; }
command -v cmp >/dev/null 2>&1 || { printf '%s\n' 'error: cmp is required' >&2; exit 1; }
command -v gzip >/dev/null 2>&1 || { printf '%s\n' 'error: gzip is required' >&2; exit 1; }
command -v head >/dev/null 2>&1 || { printf '%s\n' 'error: head is required' >&2; exit 1; }
command -v wc >/dev/null 2>&1 || { printf '%s\n' 'error: wc is required' >&2; exit 1; }
command -v awk >/dev/null 2>&1 || { printf '%s\n' 'error: awk is required' >&2; exit 1; }
[[ "$(tar --version 2>/dev/null)" == *"GNU tar"* ]] || {
    printf '%s\n' 'error: GNU tar is required for bounded archive verification' >&2
    exit 1
}

max_archive_bytes=$((64 * 1024 * 1024))
max_archive_entries=2048
max_member_bytes=$((32 * 1024 * 1024))
max_expanded_bytes=$((64 * 1024 * 1024))
max_entry_path_bytes=512
max_tar_stream_bytes=$((max_expanded_bytes + max_archive_entries * 1024 + 1024))
archive_bytes="$(wc -c < "$archive")"
[[ "$archive_bytes" =~ ^[0-9]+$ && "$archive_bytes" -le "$max_archive_bytes" ]] || {
    printf 'error: archive size exceeds maximum %s bytes\n' "$max_archive_bytes" >&2
    exit 1
}

archive_dir="$(cd -- "$(dirname -- "$archive")" && pwd -P)"
archive_name="$(basename -- "$archive")"
checksum="$archive_dir/$archive_name.sha256"
if [[ -e "$checksum" || -L "$checksum" ]]; then
    [[ -f "$checksum" && ! -L "$checksum" ]] || {
        printf '%s\n' 'error: checksum path must be a non-symlink regular file' >&2
        exit 1
    }
    checksum_bytes="$(wc -c < "$checksum")"
    [[ "$checksum_bytes" -le 512 ]] || {
        printf '%s\n' 'error: checksum file exceeds maximum 512 bytes' >&2
        exit 1
    }
    mapfile -t checksum_lines < "$checksum"
    [[ "${#checksum_lines[@]}" -eq 1 ]] || {
        printf '%s\n' 'error: checksum file must contain exactly one entry' >&2
        exit 1
    }
    read -r expected_digest expected_name checksum_extra <<< "${checksum_lines[0]}"
    [[ "$expected_digest" =~ ^[0-9a-f]{64}$ && "$expected_name" == "$archive_name" && -z "${checksum_extra:-}" ]] || {
        printf '%s\n' 'error: checksum file must name only the selected archive' >&2
        exit 1
    }
    actual_digest="$(sha256sum -- "$archive")"
    actual_digest="${actual_digest%% *}"
    [[ "$actual_digest" == "$expected_digest" ]] || {
        printf '%s\n' 'error: archive checksum does not match' >&2
        exit 1
    }
    printf '%s: OK\n' "$archive_name"
fi

verification_root="$(mktemp -d)"
cleanup() {
    rm -rf -- "$verification_root"
}
trap cleanup EXIT
bounded_tar="$verification_root/archive.tar"
set +e
gzip --decompress --stdout -- "$archive" |
    head -c "$((max_tar_stream_bytes + 1))" > "$bounded_tar"
decompression_status=$?
set -e
tar_stream_bytes="$(wc -c < "$bounded_tar")"
if [[ "$tar_stream_bytes" -gt "$max_tar_stream_bytes" ]]; then
    printf 'error: decompressed archive stream exceeds maximum %s bytes\n' \
        "$max_tar_stream_bytes" >&2
    exit 1
fi
[[ "$decompression_status" -eq 0 ]] || {
    printf '%s\n' 'error: archive gzip stream is invalid' >&2
    exit 1
}

listing="$(
    LC_ALL=C TZ=UTC tar --list --verbose --numeric-owner --quoting-style=escape \
        --file "$bounded_tar" |
        awk -v maximum="$max_archive_entries" '
            NR > maximum { exit 42 }
            { print }
        '
)" || {
    printf 'error: archive is invalid or exceeds maximum %s entries\n' "$max_archive_entries" >&2
    exit 1
}
[[ -n "$listing" ]] || { printf '%s\n' 'error: archive is empty' >&2; exit 1; }
extended_listing="$(
    LC_ALL=C TZ=UTC tar --list --ignore-zeros --verbose --numeric-owner --quoting-style=escape \
        --file "$bounded_tar" |
        awk -v maximum="$max_archive_entries" '
            NR > maximum { exit 42 }
            { print }
        '
)" || {
    printf '%s\n' 'error: archive has invalid data after its end-of-archive marker' >&2
    exit 1
}
[[ "$extended_listing" == "$listing" ]] || {
    printf '%s\n' 'error: archive contains entries after its end-of-archive marker' >&2
    exit 1
}
mapfile -t metadata_entries <<< "$listing"

declare -A entry_types=()
root=""
expanded_bytes=0
for metadata in "${metadata_entries[@]}"; do
    read -r mode owner_group size modified_date modified_time entry metadata_extra <<< "$metadata"
    [[ -n "${mode:-}" && -n "${owner_group:-}" && -n "${size:-}" \
        && -n "${modified_date:-}" && -n "${modified_time:-}" && -n "${entry:-}" ]] || {
        printf '%s\n' 'error: archive metadata is not in the required normalized form' >&2
        exit 1
    }
    entry_type="${mode:0:1}"
    [[ "$entry_type" == "d" || "$entry_type" == "-" ]] || {
        printf 'error: archive entry must be a directory or regular file: %s\n' "$entry" >&2
        exit 1
    }
    [[ "$size" =~ ^[0-9]+$ && -z "${metadata_extra:-}" ]] || {
        printf '%s\n' 'error: archive metadata is not in the required normalized form' >&2
        exit 1
    }
    [[ "$entry" =~ ^[A-Za-z0-9._/-]+$ && "${#entry}" -le "$max_entry_path_bytes" ]] || {
        printf 'error: unsafe archive entry name: %s\n' "$entry" >&2
        exit 1
    }
    entry="${entry%/}"
    [[ -n "$entry" && "$entry" != *"//"* && "$entry" != "." && "$entry" != ".." \
        && "$entry" != */./* && "$entry" != */. && "$entry" != */../* && "$entry" != */.. ]] || {
        printf 'error: unsafe archive entry: %s\n' "$entry" >&2
        exit 1
    }

    if [[ -z "$root" ]]; then
        [[ "$entry_type" == "d" ]] || {
            printf '%s\n' 'error: archive must begin with its root directory' >&2
            exit 1
        }
        root="$entry"
        [[ "$root" != */* \
            && "$root" =~ ^ba-strategy-v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?-x86_64-unknown-linux-gnu$ ]] || {
            printf 'error: unexpected archive root: %s\n' "$root" >&2
            exit 1
        }
    fi
    [[ "$entry" == "$root" || "$entry" == "$root/"* ]] || {
        printf 'error: archive entry is outside root %s: %s\n' "$root" "$entry" >&2
        exit 1
    }
    [[ -z "${entry_types[$entry]+present}" ]] || {
        printf 'error: duplicate archive entry: %s\n' "$entry" >&2
        exit 1
    }
    [[ "$owner_group" == "0/0" && "$modified_date" == "1970-01-01" \
        && "$modified_time" == "00:00" ]] || {
        printf 'error: archive ownership or timestamp is not normalized: %s\n' "$entry" >&2
        exit 1
    }
    expected_mode="-rw-r--r--"
    if [[ "$entry_type" == "d" ]]; then
        expected_mode="drwxr-xr-x"
    elif [[ "$entry" == "$root/ba-strategy" ]]; then
        expected_mode="-rwxr-xr-x"
    fi
    [[ "$mode" == "$expected_mode" ]] || {
        printf 'error: archive mode is not normalized for %s\n' "$entry" >&2
        exit 1
    }
    entry_types["$entry"]="$entry_type"

    if [[ "$entry_type" == "d" ]]; then
        [[ "$size" -eq 0 ]] || {
            printf 'error: archive directory has a nonzero declared size: %s\n' "$entry" >&2
            exit 1
        }
    else
        [[ "$size" -le "$max_member_bytes" ]] || {
            printf 'error: archive member exceeds maximum %s bytes: %s\n' "$max_member_bytes" "$entry" >&2
            exit 1
        }
        expanded_bytes=$((expanded_bytes + size))
        [[ "$expanded_bytes" -le "$max_expanded_bytes" ]] || {
            printf 'error: archive payload exceeds maximum %s bytes\n' "$max_expanded_bytes" >&2
            exit 1
        }
    fi

    [[ "$entry" != "$root/tests/fixtures" && "$entry" != "$root/tests/fixtures/"* ]] || {
        printf 'error: test fixtures must not be included in runtime archive: %s\n' "$entry" >&2
        exit 1
    }
done

require_entry() {
    local path="$root/$1"
    [[ "${entry_types[$path]:-}" == "-" ]] && return
    printf 'error: archive is missing %s\n' "$path" >&2
    exit 1
}

required_paths=(
    ba-strategy RELEASE-MANIFEST.sha256 README.md README.zh-CN.md CHANGELOG.md SECURITY.md
    LICENSE-MIT LICENSE-APACHE
    docs/CALIBRATION.md docs/DATA_PROVENANCE.md docs/PROTOCOLS.md docs/SCHEMA_V2.md
    docs/RELEASING.md docs/SCHEMA_V3.md docs/STRATEGIES.md docs/THREAT_MODEL.md
    docs/adr/0001-deterministic-parallel-monte-carlo.md
    data/rulesets/jp_2026_07_29_provisional_v2.json
    data/rulesets/jp_2026_07_29_provisional_v3.json
    data/rewards/jp_2026_07_29_campaign_v2.json
    data/rewards/jp_2026_07_29_empty_v2.json
    data/rewards/jp_2026_07_29_empty_v3.json
)
for path in "${required_paths[@]}"; do
    require_entry "$path"
done

extract_root="$verification_root/extracted"
unrelated_dir="$verification_root/unrelated"
mkdir -- "$extract_root" "$unrelated_dir"
umask 077
tar --extract --file "$bounded_tar" --directory "$extract_root" \
    --no-same-owner --no-same-permissions --delay-directory-restore
unexpected_type="$(find "$extract_root/$root" ! -type d ! -type f -print -quit)"
[[ -z "$unexpected_type" ]] || {
    printf 'error: extracted archive contains an unexpected node type: %s\n' "$unexpected_type" >&2
    exit 1
}
for path in "${required_paths[@]}"; do
    [[ -f "$extract_root/$root/$path" && ! -L "$extract_root/$root/$path" ]] || {
        printf 'error: extracted archive is missing required regular file %s/%s\n' \
            "$root" "$path" >&2
        exit 1
    }
done
actual_manifest="$extract_root/ACTUAL-MANIFEST.sha256"
(
    cd "$extract_root/$root"
    find . -type f ! -path ./RELEASE-MANIFEST.sha256 -print0 |
        sort -z |
        xargs -0 --no-run-if-empty sha256sum
) > "$actual_manifest"
if ! cmp --silent -- "$extract_root/$root/RELEASE-MANIFEST.sha256" "$actual_manifest"; then
    printf '%s\n' 'error: release manifest does not exactly cover every packaged regular file' >&2
    exit 1
fi
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
        "$binary" --data-dir "$extract_root/$root/data" --scenario-dir "$extract_root/$root/scenarios/golden" \
            analyze v3_three_target_exact_small --format json >/dev/null
    )
fi
