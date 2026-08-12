#!/usr/bin/env bash
# Assemble a deterministic Linux runtime archive from an already-built binary.
set -euo pipefail

usage() {
    printf 'usage: %s --version <version> --target <triple> --output-dir <directory> [--tag <vversion>] [--commit <sha>]\n' "${0##*/}" >&2
}

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
version=""
target=""
output_dir=""
tag=""
commit=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version|--target|--output-dir|--tag|--commit)
            [[ $# -ge 2 ]] || { usage; exit 2; }
            case "$1" in
                --version) version="$2" ;;
                --target) target="$2" ;;
                --output-dir) output_dir="$2" ;;
                --tag) tag="$2" ;;
                --commit) commit="$2" ;;
            esac
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

[[ -n "$version" && -n "$target" && -n "$output_dir" ]] || { usage; exit 2; }
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]] || {
    printf 'error: invalid release version: %s\n' "$version" >&2
    exit 2
}
[[ -z "$tag" || "$tag" == "v$version" ]] || {
    printf 'error: tag %s does not match version %s\n' "$tag" "$version" >&2
    exit 1
}

manifest_version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo_root/Cargo.toml" | head -n 1)"
[[ "$manifest_version" == "$version" ]] || {
    printf 'error: requested version %s does not match Cargo.toml version %s\n' "$version" "${manifest_version:-<missing>}" >&2
    exit 1
}

if [[ -z "$commit" ]] && git -C "$repo_root" rev-parse --verify HEAD >/dev/null 2>&1; then
    commit="$(git -C "$repo_root" rev-parse HEAD)"
fi
[[ -n "$commit" ]] || {
    printf '%s\n' 'error: release commit is required outside a Git checkout; pass --commit <sha>' >&2
    exit 1
}
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || {
    printf 'error: commit must be a full lowercase SHA-1: %s\n' "$commit" >&2
    exit 2
}

binary="$repo_root/target/$target/release/ba-strategy"
if [[ ! -x "$binary" ]]; then
    printf 'error: release binary is missing or not executable: %s\n' "$binary" >&2
    printf 'hint: cargo build --release --locked -p ba-cli --bin ba-strategy --target %s\n' "$target" >&2
    exit 1
fi

required_files=(
    README.md CHANGELOG.md SECURITY.md LICENSE-MIT LICENSE-APACHE
    docs/CALIBRATION.md docs/COMPATIBILITY.md docs/PROTOCOLS.md docs/SCHEMA_V1.md
    docs/SCHEMA_V2.md docs/STRATEGIES.md docs/THREAT_MODEL.md
    data/rulesets/jp_2026_07_29_provisional_v1.json
    data/rulesets/jp_2026_07_29_provisional_v2.json
    data/rewards/empty_v1.json
    data/rewards/jp_2026_07_29_campaign_v1.json
    data/rewards/jp_2026_07_29_empty_v2.json
)
for relative_path in "${required_files[@]}"; do
    [[ -f "$repo_root/$relative_path" ]] || {
        printf 'error: required release file is missing: %s\n' "$relative_path" >&2
        exit 1
    }
done
[[ -d "$repo_root/scenarios/golden" && -d "$repo_root/scenarios/examples" ]] || {
    printf '%s\n' 'error: release requires scenarios/golden and scenarios/examples directories' >&2
    exit 1
}

mkdir -p -- "$output_dir"
output_dir="$(cd -- "$output_dir" && pwd -P)"
base_name="ba-strategy-v${version}-${target}"
archive="$output_dir/$base_name.tar.gz"
checksum="$archive.sha256"
metadata="$output_dir/$base_name.release-metadata"
stage_root="$(mktemp -d)"
cleanup() {
    rm -rf -- "$stage_root"
}
trap cleanup EXIT

stage="$stage_root/$base_name"
mkdir -p -- "$stage/data/rulesets" "$stage/data/rewards" "$stage/scenarios" "$stage/docs"
install -m 0755 -- "$binary" "$stage/ba-strategy"
cp -- "$repo_root/data/rulesets/"*.json "$stage/data/rulesets/"
cp -- "$repo_root/data/rewards/"*.json "$stage/data/rewards/"
cp -R -- "$repo_root/scenarios/golden" "$stage/scenarios/golden"
cp -R -- "$repo_root/scenarios/examples" "$stage/scenarios/examples"
cp -- "$repo_root/README.md" "$repo_root/CHANGELOG.md" "$repo_root/SECURITY.md" \
    "$repo_root/LICENSE-MIT" "$repo_root/LICENSE-APACHE" "$stage/"
cp -- "$repo_root/docs/CALIBRATION.md" "$repo_root/docs/COMPATIBILITY.md" \
    "$repo_root/docs/PROTOCOLS.md" "$repo_root/docs/SCHEMA_V1.md" "$repo_root/docs/SCHEMA_V2.md" \
    "$repo_root/docs/STRATEGIES.md" "$repo_root/docs/THREAT_MODEL.md" "$stage/docs/"

find "$stage" -type d -exec chmod 0755 {} +
find "$stage" -type f -exec chmod 0644 {} +
chmod 0755 -- "$stage/ba-strategy"

(
    cd "$stage"
    find . -type f ! -name RELEASE-MANIFEST.sha256 -print0 |
        sort -z |
        xargs -0 sha256sum > RELEASE-MANIFEST.sha256
)

rm -f -- "$archive" "$checksum" "$metadata"
if tar --version 2>/dev/null | head -n 1 | rg -q 'GNU tar'; then
    tar --create --gzip --file "$archive" --sort=name --mtime='@0' --owner=0 --group=0 --numeric-owner \
        --directory "$stage_root" "$base_name"
else
    printf '%s\n' 'error: GNU tar is required to create normalized release archives' >&2
    exit 1
fi

(
    cd "$output_dir"
    sha256sum "${archive##*/}" > "${checksum##*/}"
)
{
    printf 'version=%s\n' "$version"
    printf 'tag=%s\n' "$tag"
    printf 'target=%s\n' "$target"
    printf 'commit=%s\n' "$commit"
    printf 'archive=%s\n' "${archive##*/}"
    printf 'archive_sha256=%s\n' "$(sha256sum "$archive" | awk '{print $1}')"
} > "$metadata"

printf '%s\n' "$archive"
